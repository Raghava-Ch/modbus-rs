/*
 * c_gateway_demo — End-to-end Modbus TCP→TCP gateway using mbus-ffi C bindings.
 *
 *   client   ─TCP→  127.0.0.1:5020  [gateway]  ─TCP→  127.0.0.1:15020  [echo srv]
 *
 * The "echo server" is a tiny in-process Modbus TCP responder that handles
 * Function Code 0x03 (Read Holding Registers) by returning register[i] = i.
 *
 * The C app:
 *   - implements all four lock hooks (mbus_pool_lock/unlock, mbus_gateway_lock/unlock)
 *   - implements two MbusTransportCallbacks (upstream listener, downstream client)
 *   - implements one MbusGatewayCallbacks (event observability)
 *   - calls mbus_gateway_poll() in a loop until idle
 */

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>

#include "modbus_rs_gateway.h"

#define UPSTREAM_PORT   5020
#define DOWNSTREAM_PORT 15020
#define IDLE_TIMEOUT_MS 30000

/* ───────────────────────── Lock hooks ────────────────────────────────────── */

static pthread_mutex_t g_pool_mutex = PTHREAD_MUTEX_INITIALIZER;
void mbus_pool_lock(void)   { pthread_mutex_lock(&g_pool_mutex); }
void mbus_pool_unlock(void) { pthread_mutex_unlock(&g_pool_mutex); }

#define MAX_GATEWAYS_LOCAL 8
static pthread_mutex_t g_gw_mutexes[MAX_GATEWAYS_LOCAL] = { PTHREAD_MUTEX_INITIALIZER };
void mbus_gateway_lock(uint8_t id) {
    if (id < MAX_GATEWAYS_LOCAL) pthread_mutex_lock(&g_gw_mutexes[id]);
}
void mbus_gateway_unlock(uint8_t id) {
    if (id < MAX_GATEWAYS_LOCAL) pthread_mutex_unlock(&g_gw_mutexes[id]);
}

/* ───────────────────────── Echo Modbus server (downstream) ──────────────── */

static volatile sig_atomic_t g_stop = 0;
static void on_sigint(int s) { (void)s; g_stop = 1; }

static int set_nonblocking(int fd) {
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) return -1;
    return fcntl(fd, F_SETFL, flags | O_NONBLOCK);
}

static void *echo_server_thread(void *arg) {
    (void)arg;
    int srv = socket(AF_INET, SOCK_STREAM, 0);
    int yes = 1;
    setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes));
    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_port = htons(DOWNSTREAM_PORT);
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(srv, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        perror("echo bind");
        close(srv);
        return NULL;
    }
    listen(srv, 4);
    set_nonblocking(srv);

    fprintf(stderr, "[echo-srv] listening on 127.0.0.1:%d\n", DOWNSTREAM_PORT);

    while (!g_stop) {
        struct sockaddr_in peer;
        socklen_t plen = sizeof(peer);
        int cli = accept(srv, (struct sockaddr*)&peer, &plen);
        if (cli < 0) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                struct timespec ts = {0, 50 * 1000 * 1000};
                nanosleep(&ts, NULL);
                continue;
            }
            break;
        }
        fprintf(stderr, "[echo-srv] downstream connection accepted\n");
        /* Force blocking mode (accepted sockets inherit O_NONBLOCK on macOS). */
        int flags = fcntl(cli, F_GETFL, 0);
        if (flags >= 0) fcntl(cli, F_SETFL, flags & ~O_NONBLOCK);

        uint8_t buf[260];
        while (!g_stop) {
            /* Read a complete MBAP frame: header (7 bytes) + N data bytes */
            size_t have = 0;
            while (have < 7) {
                ssize_t n = recv(cli, buf + have, 7 - have, 0);
                if (n <= 0) goto close_cli;
                have += (size_t)n;
            }
            uint16_t len_field = (uint16_t)((buf[4] << 8) | buf[5]);
            size_t total = 6 + len_field; /* header through end of data */
            while (have < total && have < sizeof(buf)) {
                ssize_t n = recv(cli, buf + have, total - have, 0);
                if (n <= 0) goto close_cli;
                have += (size_t)n;
            }

            uint16_t txn = (uint16_t)((buf[0] << 8) | buf[1]);
            uint8_t  unit = buf[6];
            uint8_t  fc   = buf[7];

            uint8_t resp[260];
            size_t  resp_len = 0;
            if (fc == 0x03) {
                /* Read Holding Registers — buf[8..9]=start, buf[10..11]=qty */
                uint16_t start = (uint16_t)((buf[8]  << 8) | buf[9]);
                uint16_t qty   = (uint16_t)((buf[10] << 8) | buf[11]);
                if (qty == 0 || qty > 125) qty = 1;
                uint16_t pdu_len = (uint16_t)(2 + qty * 2);
                resp[0] = (uint8_t)(txn >> 8);
                resp[1] = (uint8_t)(txn & 0xFF);
                resp[2] = 0; resp[3] = 0;                       /* protocol */
                uint16_t mbap_len = (uint16_t)(1 + pdu_len);    /* unit + pdu */
                resp[4] = (uint8_t)(mbap_len >> 8);
                resp[5] = (uint8_t)(mbap_len & 0xFF);
                resp[6] = unit;
                resp[7] = fc;
                resp[8] = (uint8_t)(qty * 2);
                for (uint16_t i = 0; i < qty; i++) {
                    uint16_t val = (uint16_t)(start + i);
                    resp[9 + i*2]     = (uint8_t)(val >> 8);
                    resp[9 + i*2 + 1] = (uint8_t)(val & 0xFF);
                }
                resp_len = 9 + qty * 2;
            } else {
                /* Exception: illegal function */
                resp[0] = (uint8_t)(txn >> 8);
                resp[1] = (uint8_t)(txn & 0xFF);
                resp[2] = 0; resp[3] = 0;
                resp[4] = 0; resp[5] = 3;
                resp[6] = unit;
                resp[7] = (uint8_t)(fc | 0x80);
                resp[8] = 0x01;
                resp_len = 9;
            }

            ssize_t sent = 0;
            while (sent < (ssize_t)resp_len) {
                ssize_t n = send(cli, resp + sent, resp_len - sent, 0);
                if (n <= 0) goto close_cli;
                sent += n;
            }
            fprintf(stderr, "[echo-srv] responded fc=0x%02X len=%zu\n", fc, resp_len);
        }
close_cli:
        close(cli);
    }
    close(srv);
    return NULL;
}

/* ───────────────────────── Upstream transport (TCP listener) ────────────── */

typedef struct {
    int listen_fd;
    int client_fd;          /* -1 when no upstream client connected */
} UpstreamCtx;

static MbusStatusCode up_connect(void *ud) {
    UpstreamCtx *ctx = (UpstreamCtx*)ud;
    if (ctx->listen_fd >= 0) return MbusOk;
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return MbusErrConnectionFailed;
    int yes = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes));
    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_port = htons(UPSTREAM_PORT);
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    if (bind(fd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        close(fd); return MbusErrConnectionFailed;
    }
    listen(fd, 1);
    set_nonblocking(fd);
    ctx->listen_fd = fd;
    ctx->client_fd = -1;
    fprintf(stderr, "[upstream] listening on 0.0.0.0:%d\n", UPSTREAM_PORT);
    return MbusOk;
}

static MbusStatusCode up_disconnect(void *ud) {
    UpstreamCtx *ctx = (UpstreamCtx*)ud;
    if (ctx->client_fd >= 0) { close(ctx->client_fd); ctx->client_fd = -1; }
    if (ctx->listen_fd >= 0) { close(ctx->listen_fd); ctx->listen_fd = -1; }
    return MbusOk;
}

static MbusStatusCode up_send(const uint8_t *data, uint16_t len, void *ud) {
    UpstreamCtx *ctx = (UpstreamCtx*)ud;
    if (ctx->client_fd < 0) return MbusErrConnectionClosed;
    size_t sent = 0;
    while (sent < len) {
        ssize_t n = send(ctx->client_fd, data + sent, len - sent, 0);
        if (n <= 0) return MbusErrSendFailed;
        sent += (size_t)n;
    }
    return MbusOk;
}

static MbusStatusCode up_recv(uint8_t *buf, uint16_t cap, uint16_t *out_len, void *ud) {
    UpstreamCtx *ctx = (UpstreamCtx*)ud;
    *out_len = 0;
    if (ctx->client_fd < 0) {
        struct pollfd pfd = { .fd = ctx->listen_fd, .events = POLLIN };
        if (poll(&pfd, 1, 25) <= 0) return MbusOk;
        struct sockaddr_in peer;
        socklen_t plen = sizeof(peer);
        int cli = accept(ctx->listen_fd, (struct sockaddr*)&peer, &plen);
        if (cli < 0) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) return MbusOk;
            return MbusErrConnectionFailed;
        }
        set_nonblocking(cli);
        ctx->client_fd = cli;
        fprintf(stderr, "[upstream] client connected\n");
        return MbusOk;
    }
    struct pollfd pfd = { .fd = ctx->client_fd, .events = POLLIN };
    if (poll(&pfd, 1, 25) <= 0) return MbusOk;
    ssize_t n = recv(ctx->client_fd, buf, cap, 0);
    if (n == 0) {
        close(ctx->client_fd); ctx->client_fd = -1;
        fprintf(stderr, "[upstream] client closed\n");
        return MbusErrConnectionClosed;
    }
    if (n < 0) {
        if (errno == EAGAIN || errno == EWOULDBLOCK) return MbusOk;
        return MbusErrIoError;
    }
    *out_len = (uint16_t)n;
    return MbusOk;
}

static uint8_t up_is_connected(void *ud) {
    UpstreamCtx *ctx = (UpstreamCtx*)ud;
    return ctx->listen_fd >= 0 ? 1 : 0;
}

/* ───────────────────────── Downstream transport (TCP client) ────────────── */

typedef struct {
    int      fd;
    uint16_t port;
} DownstreamCtx;

static MbusStatusCode down_connect(void *ud) {
    DownstreamCtx *ctx = (DownstreamCtx*)ud;
    if (ctx->fd >= 0) return MbusOk;
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return MbusErrConnectionFailed;
    int yes = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &yes, sizeof(yes));
    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_port = htons(ctx->port);
    inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr);

    /* Retry connect for up to ~1 s — echo server may not be ready yet. */
    for (int i = 0; i < 20; i++) {
        if (connect(fd, (struct sockaddr*)&addr, sizeof(addr)) == 0) {
            ctx->fd = fd;
            set_nonblocking(fd);
            fprintf(stderr, "[downstream] connected to 127.0.0.1:%d\n", ctx->port);
            return MbusOk;
        }
        struct timespec ts = {0, 50 * 1000 * 1000};
        nanosleep(&ts, NULL);
    }
    close(fd);
    return MbusErrConnectionFailed;
}

static MbusStatusCode down_disconnect(void *ud) {
    DownstreamCtx *ctx = (DownstreamCtx*)ud;
    if (ctx->fd >= 0) { close(ctx->fd); ctx->fd = -1; }
    return MbusOk;
}

static MbusStatusCode down_send(const uint8_t *data, uint16_t len, void *ud) {
    DownstreamCtx *ctx = (DownstreamCtx*)ud;
    if (ctx->fd < 0) {
        if (down_connect(ud) != MbusOk) return MbusErrConnectionFailed;
    }
    size_t sent = 0;
    while (sent < len) {
        ssize_t n = send(ctx->fd, data + sent, len - sent, 0);
        if (n <= 0) return MbusErrSendFailed;
        sent += (size_t)n;
    }
    return MbusOk;
}

static MbusStatusCode down_recv(uint8_t *buf, uint16_t cap, uint16_t *out_len, void *ud) {
    DownstreamCtx *ctx = (DownstreamCtx*)ud;
    *out_len = 0;
    if (ctx->fd < 0) return MbusErrConnectionClosed;
    /* Wait briefly for data so the gateway's bounded recv-retry loop can drain
     * the response without busy-spinning the CPU. */
    struct pollfd pfd = { .fd = ctx->fd, .events = POLLIN };
    int pr = poll(&pfd, 1, 25 /* ms */);
    if (pr <= 0) return MbusOk;
    ssize_t n = recv(ctx->fd, buf, cap, 0);
    if (n == 0) {
        close(ctx->fd); ctx->fd = -1;
        return MbusErrConnectionClosed;
    }
    if (n < 0) {
        if (errno == EAGAIN || errno == EWOULDBLOCK) return MbusOk;
        return MbusErrIoError;
    }
    *out_len = (uint16_t)n;
    return MbusOk;
}

static uint8_t down_is_connected(void *ud) {
    DownstreamCtx *ctx = (DownstreamCtx*)ud;
    return ctx->fd >= 0 ? 1 : 0;
}

/* ───────────────────────── Event callbacks ──────────────────────────────── */

static void on_forward(uint8_t session_id, uint8_t unit_id, uint16_t channel_idx, void *ud) {
    (void)ud;
    fprintf(stderr, "[event] on_forward(session=%u unit=%u channel=%u)\n",
            session_id, unit_id, channel_idx);
}
static void on_response_returned(uint8_t session_id, uint16_t txn, void *ud) {
    (void)ud;
    fprintf(stderr, "[event] on_response_returned(session=%u txn=%u)\n", session_id, txn);
}
static void on_routing_miss(uint8_t session_id, uint8_t unit_id, void *ud) {
    (void)ud;
    fprintf(stderr, "[event] on_routing_miss(session=%u unit=%u)\n", session_id, unit_id);
}
static void on_downstream_timeout(uint8_t session_id, uint16_t txn, void *ud) {
    (void)ud;
    fprintf(stderr, "[event] on_downstream_timeout(session=%u txn=%u)\n", session_id, txn);
}
static void on_upstream_disconnect(uint8_t session_id, void *ud) {
    (void)ud;
    fprintf(stderr, "[event] on_upstream_disconnect(session=%u)\n", session_id);
}

/* ───────────────────────── main ─────────────────────────────────────────── */

static uint64_t now_ms(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (uint64_t)tv.tv_sec * 1000ULL + (uint64_t)tv.tv_usec / 1000ULL;
}

int main(void) {
    signal(SIGINT, on_sigint);
    signal(SIGPIPE, SIG_IGN);

    /* 1. Start the downstream echo server in a background thread. */
    pthread_t echo_thread;
    pthread_create(&echo_thread, NULL, echo_server_thread, NULL);

    /* 2. Construct upstream + downstream contexts and callback tables. */
    UpstreamCtx upstream_ctx = { .listen_fd = -1, .client_fd = -1 };
    DownstreamCtx downstream_ctx = { .fd = -1, .port = DOWNSTREAM_PORT };

    MbusTransportCallbacks upstream_cb = {
        .userdata = &upstream_ctx,
        .on_connect      = up_connect,
        .on_disconnect   = up_disconnect,
        .on_send         = up_send,
        .on_recv         = up_recv,
        .on_is_connected = up_is_connected,
    };
    MbusTransportCallbacks downstream_cb = {
        .userdata = &downstream_ctx,
        .on_connect      = down_connect,
        .on_disconnect   = down_disconnect,
        .on_send         = down_send,
        .on_recv         = down_recv,
        .on_is_connected = down_is_connected,
    };
    MbusGatewayCallbacks events = {
        .userdata              = NULL,
        .on_forward            = on_forward,
        .on_response_returned  = on_response_returned,
        .on_routing_miss       = on_routing_miss,
        .on_downstream_timeout = on_downstream_timeout,
        .on_upstream_disconnect = on_upstream_disconnect,
    };

    /* 3. Bind upstream listener BEFORE handing it to the gateway. */
    if (up_connect(&upstream_ctx) != MbusOk) {
        fprintf(stderr, "failed to bind upstream listener\n");
        return 1;
    }
    /* Connect downstream client — the echo server should be ready. */
    if (down_connect(&downstream_ctx) != MbusOk) {
        fprintf(stderr, "failed to connect downstream\n");
        return 1;
    }

    /* 4. Create the gateway. */
    MbusGatewayId gw_id = MBUS_INVALID_GATEWAY_ID;
    enum MbusStatusCode rc = mbus_gateway_new(&upstream_cb, &events, &gw_id);
    if (rc != MbusOk) {
        fprintf(stderr, "mbus_gateway_new failed: %s\n", mbus_status_str(rc));
        return 1;
    }

    /* 5. Add downstream channel + route unit 1 → channel 0. */
    uint16_t channel = 0xFFFF;
    rc = mbus_gateway_add_downstream(gw_id, &downstream_cb, &channel);
    if (rc != MbusOk) {
        fprintf(stderr, "add_downstream failed: %s\n", mbus_status_str(rc));
        return 1;
    }
    rc = mbus_gateway_add_unit_route(gw_id, 1, channel);
    if (rc != MbusOk) {
        fprintf(stderr, "add_unit_route failed: %s\n", mbus_status_str(rc));
        return 1;
    }
    fprintf(stderr, "[gateway] ready — unit 1 → channel %u\n", channel);
    fprintf(stderr, "[gateway] poll loop running, send a Modbus TCP request to 127.0.0.1:%d\n",
            UPSTREAM_PORT);

    /* 6. Poll loop. */
    uint64_t deadline = now_ms() + IDLE_TIMEOUT_MS;
    while (!g_stop && now_ms() < deadline) {
        rc = mbus_gateway_poll(gw_id);
        if (rc != MbusOk) {
            fprintf(stderr, "[gateway] poll: %s\n", mbus_status_str(rc));
        }
        struct timespec ts = {0, 5 * 1000 * 1000}; /* 5 ms */
        nanosleep(&ts, NULL);
    }

    /* 7. Tear down. */
    fprintf(stderr, "[gateway] shutting down\n");
    mbus_gateway_free(gw_id);
    g_stop = 1;
    pthread_join(echo_thread, NULL);
    return 0;
}
