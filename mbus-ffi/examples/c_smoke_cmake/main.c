#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <termios.h>
#include <unistd.h>

#ifdef __APPLE__
#include <util.h>
#else
#include <pty.h>
#endif

#include "mbus_ffi.h"

static pthread_mutex_t g_pool_mutex = PTHREAD_MUTEX_INITIALIZER;

void mbus_pool_lock(void) {
    pthread_mutex_lock(&g_pool_mutex);
}

void mbus_pool_unlock(void) {
    pthread_mutex_unlock(&g_pool_mutex);
}

#define MAX_CLIENTS 64
static pthread_mutex_t g_client_mutexes[MAX_CLIENTS] = { PTHREAD_MUTEX_INITIALIZER };

void mbus_client_lock(uint8_t id) {
    if (id < MAX_CLIENTS) {
        pthread_mutex_lock(&g_client_mutexes[id]);
    }
}

void mbus_client_unlock(uint8_t id) {
    if (id < MAX_CLIENTS) {
        pthread_mutex_unlock(&g_client_mutexes[id]);
    }
}

struct TcpContext {
    int fd;
    const char *host;
    int port;
};

struct SerialContext {
    int fd;
    char slave_path[128];
};

struct SerialServerArgs {
    int master_fd;
    volatile int stop;
};

static volatile int g_request_done = 0;
static volatile int g_request_failed = 0;

static uint64_t current_millis_impl(void *userdata) {
    (void)userdata;
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (uint64_t)tv.tv_sec * 1000u + (uint64_t)tv.tv_usec / 1000u;
}

static uint16_t modbus_crc16(const uint8_t *data, size_t len) {
    uint16_t crc = 0xFFFF;
    size_t i;
    for (i = 0; i < len; i++) {
        crc ^= data[i];
        for (int bit = 0; bit < 8; bit++) {
            if (crc & 1) {
                crc = (uint16_t)((crc >> 1) ^ 0xA001);
            } else {
                crc >>= 1;
            }
        }
    }
    return crc;
}

static int write_all(int fd, const uint8_t *data, size_t len) {
    size_t written = 0;
    while (written < len) {
        ssize_t rc = write(fd, data + written, len - written);
        if (rc < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        written += (size_t)rc;
    }
    return 0;
}

static int configure_raw_fd(int fd) {
    struct termios tio;
    if (tcgetattr(fd, &tio) != 0) {
        return -1;
    }
    cfmakeraw(&tio);
    tio.c_cflag |= (CLOCAL | CREAD);
    return tcsetattr(fd, TCSANOW, &tio);
}

static enum MbusStatusCode tcp_connect(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd >= 0) {
        return MbusOk;
    }

    ctx->fd = socket(AF_INET, SOCK_STREAM, 0);
    if (ctx->fd < 0) {
        return MbusErrConnectionFailed;
    }

    int flags = fcntl(ctx->fd, F_GETFL, 0);
    if (flags >= 0) {
        fcntl(ctx->fd, F_SETFL, flags | O_NONBLOCK);
    }

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((uint16_t)ctx->port);
    if (inet_pton(AF_INET, ctx->host, &addr.sin_addr) <= 0) {
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    if (connect(ctx->fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        if (errno != EINPROGRESS && errno != EINTR) {
            close(ctx->fd);
            ctx->fd = -1;
            return MbusErrConnectionFailed;
        }
    }

    fd_set fdset;
    struct timeval tv;
    FD_ZERO(&fdset);
    FD_SET(ctx->fd, &fdset);
    tv.tv_sec = 2;
    tv.tv_usec = 0;

    int sel_res = select(ctx->fd + 1, NULL, &fdset, NULL, &tv);
    if (sel_res != 1) {
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    int so_error = 0;
    socklen_t len = sizeof(so_error);
    if (getsockopt(ctx->fd, SOL_SOCKET, SO_ERROR, &so_error, &len) < 0 || so_error != 0) {
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    printf("[tcp] connected to %s:%d\n", ctx->host, ctx->port);
    return MbusOk;
}

static enum MbusStatusCode tcp_disconnect(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd >= 0) {
        close(ctx->fd);
        ctx->fd = -1;
    }
    return MbusOk;
}

static enum MbusStatusCode tcp_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd < 0) {
        return MbusErrConnectionClosed;
    }
    if (send(ctx->fd, data, len, 0) < 0) {
        return MbusErrIoError;
    }
    return MbusOk;
}

static enum MbusStatusCode tcp_recv(uint8_t *buffer, uint16_t buffer_cap, uint16_t *out_len, void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd < 0) {
        return MbusErrConnectionClosed;
    }

    ssize_t recv_len = recv(ctx->fd, buffer, buffer_cap, 0);
    if (recv_len < 0) {
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            *out_len = 0;
            return MbusOk;
        }
        return MbusErrIoError;
    }
    if (recv_len == 0) {
        return MbusErrConnectionClosed;
    }

    *out_len = (uint16_t)recv_len;
    return MbusOk;
}

static uint8_t tcp_is_connected(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    return ctx->fd >= 0 ? 1 : 0;
}

static enum MbusStatusCode serial_connect(void *userdata) {
    struct SerialContext *ctx = (struct SerialContext *)userdata;
    if (ctx->fd >= 0) {
        return MbusOk;
    }

    ctx->fd = open(ctx->slave_path, O_RDWR | O_NOCTTY | O_NONBLOCK);
    if (ctx->fd < 0) {
        perror("open slave pty");
        return MbusErrConnectionFailed;
    }

    if (configure_raw_fd(ctx->fd) != 0) {
        perror("configure slave pty");
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    printf("[serial] opened PTY slave %s\n", ctx->slave_path);
    return MbusOk;
}

static enum MbusStatusCode serial_disconnect(void *userdata) {
    struct SerialContext *ctx = (struct SerialContext *)userdata;
    if (ctx->fd >= 0) {
        close(ctx->fd);
        ctx->fd = -1;
    }
    return MbusOk;
}

static enum MbusStatusCode serial_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct SerialContext *ctx = (struct SerialContext *)userdata;
    if (ctx->fd < 0) {
        return MbusErrConnectionClosed;
    }
    return write_all(ctx->fd, data, len) == 0 ? MbusOk : MbusErrIoError;
}

static enum MbusStatusCode serial_recv(uint8_t *buffer, uint16_t buffer_cap, uint16_t *out_len, void *userdata) {
    struct SerialContext *ctx = (struct SerialContext *)userdata;
    if (ctx->fd < 0) {
        return MbusErrConnectionClosed;
    }

    ssize_t recv_len = read(ctx->fd, buffer, buffer_cap);
    if (recv_len < 0) {
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            *out_len = 0;
            return MbusOk;
        }
        return MbusErrIoError;
    }

    *out_len = (uint16_t)recv_len;
    return MbusOk;
}

static uint8_t serial_is_connected(void *userdata) {
    struct SerialContext *ctx = (struct SerialContext *)userdata;
    return ctx->fd >= 0 ? 1 : 0;
}

static void *serial_server_thread(void *userdata) {
    struct SerialServerArgs *args = (struct SerialServerArgs *)userdata;
    uint8_t request[8];
    size_t received = 0;

    while (!args->stop && received < sizeof(request)) {
        ssize_t rc = read(args->master_fd, request + received, sizeof(request) - received);
        if (rc > 0) {
            received += (size_t)rc;
            continue;
        }
        if (rc < 0 && (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR)) {
            usleep(1000);
            continue;
        }
        break;
    }

    if (received == sizeof(request) && request[1] == 0x01) {
        uint16_t quantity = (uint16_t)((request[4] << 8) | request[5]);
        uint8_t byte_count = (uint8_t)((quantity + 7u) / 8u);
        uint8_t response[32];
        size_t response_len = 3u + byte_count;

        response[0] = request[0];
        response[1] = request[1];
        response[2] = byte_count;
        for (uint8_t i = 0; i < byte_count; i++) {
            response[3 + i] = (i == 0) ? 0x55 : 0x01;
        }

        uint16_t crc = modbus_crc16(response, response_len);
        response[response_len++] = (uint8_t)(crc & 0xFF);
        response[response_len++] = (uint8_t)(crc >> 8);

        if (write_all(args->master_fd, response, response_len) == 0) {
            printf("[serial-server] responded to read-coils request on PTY master\n");
        }
    }

    return NULL;
}

static void on_read_coils(const struct MbusReadCoilsCtx *ctx) {
    uint16_t count = mbus_coils_quantity(ctx->coils);
    uint16_t base = mbus_coils_from_address(ctx->coils);
    printf("[app] read coils response txn=%u unit=%u count=%u\n", ctx->txn_id, ctx->unit_id, count);

    for (uint16_t i = 0; i < count; i++) {
        bool val = false;
        if (mbus_coils_value_at_index(ctx->coils, i, &val) == MbusOk) {
            printf("[app] coil[%u] addr=%u value=%u\n", i, (unsigned)(base + i), val ? 1u : 0u);
        }
    }

    g_request_done = 1;
}

static void on_request_failed(const struct MbusRequestFailedCtx *ctx) {
    printf("[app] request failed txn=%u unit=%u err=%s\n",
           ctx->txn_id,
           ctx->unit_id,
           mbus_status_str(ctx->error));
    g_request_failed = 1;
    g_request_done = 1;
}

static struct MbusCallbacks build_app_callbacks(void) {
    struct MbusCallbacks callbacks;
    memset(&callbacks, 0, sizeof(callbacks));
    callbacks.on_current_millis = current_millis_impl;
    callbacks.on_request_failed = on_request_failed;
    callbacks.on_read_coils = on_read_coils;
    return callbacks;
}

static int drive_tcp_smoke(const char *host, int port) {
    struct TcpContext ctx;
    memset(&ctx, 0, sizeof(ctx));
    ctx.fd = -1;
    ctx.host = host;
    ctx.port = port;

    struct MbusTransportCallbacks transport;
    memset(&transport, 0, sizeof(transport));
    transport.userdata = &ctx;
    transport.on_connect = tcp_connect;
    transport.on_disconnect = tcp_disconnect;
    transport.on_send = tcp_send;
    transport.on_recv = tcp_recv;
    transport.on_is_connected = tcp_is_connected;

    struct MbusCallbacks callbacks = build_app_callbacks();

    struct MbusTcpConfig config;
    memset(&config, 0, sizeof(config));
    config.host = host;
    config.port = (uint16_t)port;
    config.connection_timeout_ms = 2000;
    config.response_timeout_ms = 2000;
    config.retries = 1;
    config.backoff_strategy = MbusBackoffImmediate;

    g_request_done = 0;
    g_request_failed = 0;

    MbusClientId client_id = mbus_tcp_client_new(&config, &transport, &callbacks);
    if (client_id == MBUS_INVALID_CLIENT_ID) {
        fprintf(stderr, "failed to create TCP client\n");
        return 1;
    }

    enum MbusStatusCode status = mbus_tcp_connect(client_id);
    if (status != MbusOk) {
        fprintf(stderr, "mbus_tcp_connect failed: %s\n", mbus_status_str(status));
        mbus_tcp_client_free(client_id);
        return 1;
    }

    status = mbus_tcp_read_coils(client_id, 42, 1, 0, 10);
    if (status != MbusOk) {
        fprintf(stderr, "mbus_tcp_read_coils failed: %s\n", mbus_status_str(status));
        mbus_tcp_client_free(client_id);
        return 1;
    }

    uint64_t deadline = current_millis_impl(NULL) + 5000;
    while (!g_request_done && current_millis_impl(NULL) < deadline) {
        mbus_tcp_poll(client_id);
        usleep(10000);
    }

    mbus_tcp_disconnect(client_id);
    mbus_tcp_client_free(client_id);

    if (!g_request_done || g_request_failed) {
        fprintf(stderr, "TCP smoke did not complete successfully\n");
        return 1;
    }

    printf("[tcp] smoke finished successfully\n");
    return 0;
}

static int drive_serial_pty_smoke(void) {
    int master_fd = -1;
    int slave_fd = -1;
    char slave_name[128] = {0};
    pthread_t server_thread;
    struct SerialServerArgs server_args;
    struct SerialContext ctx;
    struct MbusTransportCallbacks transport;
    struct MbusCallbacks callbacks;
    struct MbusSerialConfig config;
    int result = 1;

    if (openpty(&master_fd, &slave_fd, slave_name, NULL, NULL) != 0) {
        perror("openpty");
        return 1;
    }
    if (configure_raw_fd(master_fd) != 0 || configure_raw_fd(slave_fd) != 0) {
        perror("configure pty");
        goto cleanup;
    }
    if (fcntl(master_fd, F_SETFL, O_NONBLOCK) != 0) {
        perror("fcntl master");
        goto cleanup;
    }

    memset(&ctx, 0, sizeof(ctx));
    ctx.fd = -1;
    strncpy(ctx.slave_path, slave_name, sizeof(ctx.slave_path) - 1);

    memset(&server_args, 0, sizeof(server_args));
    server_args.master_fd = master_fd;
    if (pthread_create(&server_thread, NULL, serial_server_thread, &server_args) != 0) {
        perror("pthread_create");
        goto cleanup;
    }

    memset(&transport, 0, sizeof(transport));
    transport.userdata = &ctx;
    transport.on_connect = serial_connect;
    transport.on_disconnect = serial_disconnect;
    transport.on_send = serial_send;
    transport.on_recv = serial_recv;
    transport.on_is_connected = serial_is_connected;

    callbacks = build_app_callbacks();

    memset(&config, 0, sizeof(config));
    config.port_name = ctx.slave_path;
    config.baud_rate = 19200;
    config.mode = MbusSerialRtu;
    config.response_timeout_ms = 2000;
    config.retries = 1;
    config.backoff_strategy = MbusBackoffImmediate;

    g_request_done = 0;
    g_request_failed = 0;

    MbusClientId client_id = mbus_serial_client_new(&config, &transport, &callbacks);
    if (client_id == MBUS_INVALID_CLIENT_ID) {
        fprintf(stderr, "failed to create serial client\n");
        server_args.stop = 1;
        pthread_join(server_thread, NULL);
        goto cleanup;
    }

    enum MbusStatusCode status = mbus_serial_connect(client_id);
    if (status != MbusOk) {
        fprintf(stderr, "mbus_serial_connect failed: %s\n", mbus_status_str(status));
        mbus_serial_client_free(client_id);
        server_args.stop = 1;
        pthread_join(server_thread, NULL);
        goto cleanup;
    }

    status = mbus_serial_read_coils(client_id, 7, 1, 0, 10);
    if (status != MbusOk) {
        fprintf(stderr, "mbus_serial_read_coils failed: %s\n", mbus_status_str(status));
        mbus_serial_client_free(client_id);
        server_args.stop = 1;
        pthread_join(server_thread, NULL);
        goto cleanup;
    }

    uint64_t deadline = current_millis_impl(NULL) + 5000;
    while (!g_request_done && current_millis_impl(NULL) < deadline) {
        mbus_serial_poll(client_id);
        usleep(10000);
    }

    mbus_serial_disconnect(client_id);
    mbus_serial_client_free(client_id);
    server_args.stop = 1;
    pthread_join(server_thread, NULL);

    if (g_request_done && !g_request_failed) {
        printf("[serial] PTY smoke finished successfully on %s\n", ctx.slave_path);
        result = 0;
    } else {
        fprintf(stderr, "serial PTY smoke did not complete successfully\n");
    }

cleanup:
    if (slave_fd >= 0) {
        close(slave_fd);
    }
    if (master_fd >= 0) {
        close(master_fd);
    }
    return result;
}

static void print_usage(const char *argv0) {
    printf("Usage:\n");
    printf("  %s --serial-pty\n", argv0);
    printf("  %s --tcp <host> <port>\n", argv0);
    printf("\nDefault: --serial-pty\n");
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--help") == 0) {
        print_usage(argv[0]);
        return 0;
    }

    printf("Starting mbus-ffi C smoke example\n");

    if (argc > 1 && strcmp(argv[1], "--tcp") == 0) {
        const char *host = argc > 2 ? argv[2] : "127.0.0.1";
        int port = argc > 3 ? atoi(argv[3]) : 502;
        return drive_tcp_smoke(host, port);
    }

    return drive_serial_pty_smoke();
}
