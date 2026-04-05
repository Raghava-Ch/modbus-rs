#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <sys/socket.h>
#include <arpa/inet.h>
#include <sys/time.h>
#include "mbus_ffi.h"

// ── Real TCP Transport Implementation ──────────────────────────────────────

struct TcpContext {
    int fd;
    const char *host;
    int port;
};

static uint64_t current_millis_impl(void *userdata) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (uint64_t)(tv.tv_sec) * 1000 + (uint64_t)(tv.tv_usec) / 1000;
}

static enum MbusStatusCode tcp_connect(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd >= 0) return MbusOk;

    printf("[Transport] Connecting to %s:%d...\n", ctx->host, ctx->port);

    ctx->fd = socket(AF_INET, SOCK_STREAM, 0);
    if (ctx->fd < 0) {
        printf("[Transport] socket() failed: %s\n", strerror(errno));
        return MbusErrConnectionFailed;
    }

    int flags = fcntl(ctx->fd, F_GETFL, 0);
    fcntl(ctx->fd, F_SETFL, flags | O_NONBLOCK);

    struct sockaddr_in serv_addr;
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_port = htons(ctx->port);
    if (inet_pton(AF_INET, ctx->host, &serv_addr.sin_addr) <= 0) {
        printf("[Transport] inet_pton() failed\n");
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    if (connect(ctx->fd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)) < 0) {
        if (errno != EINPROGRESS && errno != EINTR) {
            printf("[Transport] connect() failed instantly: %s\n", strerror(errno));
            close(ctx->fd);
            ctx->fd = -1;
            return MbusErrConnectionFailed;
        }
    }

    fd_set fdset;
    FD_ZERO(&fdset);
    FD_SET(ctx->fd, &fdset);
    struct timeval tv;
    tv.tv_sec = 2;
    tv.tv_usec = 0;

    int sel_res = select(ctx->fd + 1, NULL, &fdset, NULL, &tv);
    if (sel_res == 1) {
        int so_error = 0;
        socklen_t len = sizeof(so_error);
        if (getsockopt(ctx->fd, SOL_SOCKET, SO_ERROR, &so_error, &len) < 0) {
            printf("[Transport] getsockopt() failed: %s\n", strerror(errno));
            close(ctx->fd);
            ctx->fd = -1;
            return MbusErrConnectionFailed;
        }
        if (so_error != 0) {
            printf("[Transport] Connection refused/failed async: %s\n", strerror(so_error));
            close(ctx->fd);
            ctx->fd = -1;
            return MbusErrConnectionFailed;
        }
    } else if (sel_res == 0) {
        printf("[Transport] Connection timed out after 2s\n");
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed; // Timeout!
    } else {
        printf("[Transport] select() failed: %s\n", strerror(errno));
        close(ctx->fd);
        ctx->fd = -1;
        return MbusErrConnectionFailed;
    }

    printf("[Transport] Connected successfully!\n");
    return MbusOk;
}

static enum MbusStatusCode tcp_disconnect(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd >= 0) {
        printf("[Transport] Disconnected\n");
        close(ctx->fd);
        ctx->fd = -1;
    }
    return MbusOk;
}

static enum MbusStatusCode tcp_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd < 0) return MbusErrConnectionClosed;

    printf("[Transport] Send %d bytes\n", len);
    ssize_t sent = send(ctx->fd, data, len, 0);
    if (sent < 0) {
        return MbusErrIoError;
    }
    return MbusOk;
}

static enum MbusStatusCode tcp_recv(uint8_t *buffer, uint16_t buffer_cap, uint16_t *out_len, void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    if (ctx->fd < 0) return MbusErrConnectionClosed;

    ssize_t recv_len = recv(ctx->fd, buffer, buffer_cap, 0); // O_NONBLOCK makes this return instantly
    if (recv_len < 0) {
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            *out_len = 0;
            return MbusOk;
        }
        return MbusErrIoError;
    } else if (recv_len == 0) {
        return MbusErrConnectionClosed;
    }

    printf("[Transport] Recv %ld bytes\n", recv_len);
    *out_len = (uint16_t)recv_len;
    return MbusOk;
}

static uint8_t tcp_is_connected(void *userdata) {
    struct TcpContext *ctx = (struct TcpContext *)userdata;
    return ctx->fd >= 0 ? 1 : 0;
}

// ── App Callbacks (Responses) ─────────────────────────────────────────────

// Global flag to cleanly exit the poll loop
static int g_request_done = 0;

static void on_read_coils(const struct MbusReadCoilsCtx *ctx) {
    uint16_t count = mbus_coils_quantity(ctx->coils);
    printf("[App] Read Coils Response (txn=%d, unit=%d, coils=%d)\n", 
            ctx->txn_id, ctx->unit_id, count);

    printf("Coils values: ");
    for (uint16_t i = 0; i < count; i++) {
        bool val = false;
        if (mbus_coils_value(ctx->coils, i, &val) == MbusOk) {
            printf("%d", val ? 1 : 0);
        }
    }
    printf("\n");
    g_request_done = 1;
}

static void on_request_failed(const struct MbusRequestFailedCtx *ctx) {
    printf("[App] Request Failed (txn=%d, err=%d)\n", ctx->txn_id, ctx->error);
    g_request_done = 1;
}

int main() {
    printf("Starting mbus-ffi C Smoke Test...\n\n");

    // Initialize custom posix socket context
    struct TcpContext ctx = {0};
    ctx.fd = -1;
    ctx.host = "192.168.55.200";
    ctx.port = 502;

    // 1. Setup transport
    struct MbusTransportCallbacks transport = {0};
    transport.userdata = &ctx;
    transport.on_connect = tcp_connect;
    transport.on_disconnect = tcp_disconnect;
    transport.on_send = tcp_send;
    transport.on_recv = tcp_recv;
    transport.on_is_connected = tcp_is_connected;

    // 2. Setup callbacks
    struct MbusCallbacks app_callbacks = {0};
    app_callbacks.userdata = NULL;
    app_callbacks.on_current_millis = current_millis_impl;
    app_callbacks.on_request_failed = on_request_failed;
    app_callbacks.on_read_coils = on_read_coils;

    struct MbusTcpConfig config = {0};
    config.host = "192.168.55.200";
    config.port = 502;
    config.connection_timeout_ms = 2000;
    config.response_timeout_ms = 2000;
    config.retries = 3;
    config.backoff_strategy = MbusBackoffImmediate;
    config.backoff_base_delay_ms = 0;
    config.backoff_max_delay_ms = 0;
    config.jitter_percent = 0;

    // 4. Create Client Entity
    MbusClientId client_id = mbus_tcp_client_new(&config, &transport, &app_callbacks);
    printf("Client created. ID: %d\n", client_id);
    if (client_id == MBUS_INVALID_CLIENT_ID) {
        printf("Failed to create client! Pool might be full.\n");
        return 1;
    }

    printf("Client created. ID: %d\n", client_id);

    // 5. Connect
    mbus_tcp_connect(client_id);

    // 6. Queue a read coils request (txn_id = 42, unit_id = 1, address = 0, quantity = 10)
    enum MbusStatusCode status = mbus_tcp_read_coils(client_id, 42, 1, 0, 10);
    if (status != MbusOk) {
        printf("mbus_tcp_read_coils failed! err: %d\n", status);
    }

    // 7. Poll the state machine until response or failure
    printf("\nPolling state machine...\n");
    while (!g_request_done) {
        mbus_tcp_poll(client_id);
        usleep(1000 * 10); // 10ms rest to avoid tight loop
    }

    // 8. Disconnect and Free
    mbus_tcp_disconnect(client_id);
    mbus_tcp_client_free(client_id);

    printf("\nSmoke test finished successfully.\n");
    return 0;
}
