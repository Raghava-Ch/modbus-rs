/*
 * c_server_demo_yaml — Modbus TCP server demo generated from YAML config.
 *
 * Usage:
 *   ./c_server_demo_yaml                Run as a real TCP server on port 5020.
 *   ./c_server_demo_yaml --self-test    In-process self-test (used by CTest).
 *
 * Register map (slave address 1):
 *   Coils        0x0000  pump_run          FC01 / FC05
 *   Coils        0x0001  alarm_ack         FC01 / FC05
 *   Holding regs 0x0000  speed_setpoint    FC03 / FC06
 *   Holding regs 0x0001  pressure_limit    FC03 / FC06
 *   Input regs   0x0000  pressure_actual   FC04 (read-only)
 *   Input regs   0x0001  temperature_act   FC04 (read-only)
 *   Discrete inp 0x0000  fault_active      FC02 (read-only)
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <pthread.h>
#include <signal.h>
#include <time.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <sys/select.h>
#include <netinet/in.h>
#include <arpa/inet.h>

#include "mbus_server_app.h"

#define SERVER_PORT  5020
#define MAX_FRAME    260

/* =========================================================================
 * Graceful shutdown flag (server mode only)
 * ========================================================================= */
static atomic_int g_running;
static void sig_handler(int sig) { (void)sig; atomic_store(&g_running, 0); }

/* =========================================================================
 * Lock shims — pthread_mutex_t implementations
 *
 * Called EXCLUSIVELY by Rust internals via the pool/server lock hooks.
 * The application must NOT call these directly.
 *
 * Lock taxonomy:
 *   s_server_pool_mutex  protects the Rust server slot pool
 *   s_server_mutex       per-server borrow guard (one server in this demo)
 *   s_app_mutex          protects Rust-owned APP_MODEL register state
 *
 * NOTE: mbus_pool_lock / mbus_client_lock are NOT required here because
 * the `c` (client) feature is not enabled in this server-only build.
 * ========================================================================= */
static pthread_mutex_t s_server_pool_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t s_server_mutex      = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t s_app_mutex         = PTHREAD_MUTEX_INITIALIZER;

void mbus_app_lock(void)                   { pthread_mutex_lock(&s_app_mutex);   }
void mbus_app_unlock(void)                 { pthread_mutex_unlock(&s_app_mutex); }
void mbus_pool_lock(void)                  { pthread_mutex_lock(&s_server_pool_mutex);   }
void mbus_pool_unlock(void)                { pthread_mutex_unlock(&s_server_pool_mutex); }
void mbus_server_lock(MbusServerId id)     { (void)id; pthread_mutex_lock(&s_server_mutex);   }
void mbus_server_unlock(MbusServerId id)   { (void)id; pthread_mutex_unlock(&s_server_mutex); }

/* =========================================================================
 * Application state
 * ========================================================================= */
struct AppState {
    bool     pump_running;
    uint32_t write_count;
};

/* =========================================================================
 * Write-notification hooks
 *
 * Called by Rust BEFORE a write is stored in APP_MODEL.
 * Return MBUS_HOOK_OK to accept, any other value to reject.
 *
 * IMPORTANT: Do NOT call mbus_server_get_* / mbus_server_set_* from here —
 * Rust already holds s_app_mutex at the call site.
 * ========================================================================= */
MbusHookStatus app_on_write_pump_run(void *ctx, uint16_t address, uint8_t value) {
    (void)address;
    struct AppState *app = (struct AppState *)ctx;
    if (!app) return MBUS_HOOK_DEVICE_FAILURE;
    app->pump_running = (value != 0u);
    app->write_count++;
    printf("[write] pump_run     = %s\n", app->pump_running ? "ON" : "OFF");
    return MBUS_HOOK_OK;
}
MbusHookStatus app_on_write_alarm_ack(void *ctx, uint16_t address, uint8_t value) {
    (void)address; (void)value;
    struct AppState *app = (struct AppState *)ctx;
    if (!app) return MBUS_HOOK_DEVICE_FAILURE;
    app->write_count++;
    printf("[write] alarm_ack    written\n");
    return MBUS_HOOK_OK;
}
MbusHookStatus app_on_write_speed_setpoint(void *ctx, uint16_t address, uint16_t value) {
    (void)address;
    struct AppState *app = (struct AppState *)ctx;
    if (!app) return MBUS_HOOK_DEVICE_FAILURE;
    app->write_count++;
    printf("[write] speed_setpt  = %u RPM\n", value);
    return MBUS_HOOK_OK;
}
MbusHookStatus app_on_write_pressure_limit(void *ctx, uint16_t address, uint16_t value) {
    (void)address;
    struct AppState *app = (struct AppState *)ctx;
    if (!app) return MBUS_HOOK_DEVICE_FAILURE;
    app->write_count++;
    printf("[write] pressure_lim = %u bar\n", value);
    return MBUS_HOOK_OK;
}

/* =========================================================================
 * Server thread
 * ========================================================================= */
struct ServerThread {
    MbusServerId id;
    atomic_int   running;
    pthread_t    tid;
};

/* =========================================================================
 * ── SELF-TEST MODE ─────────────────────────────────────────────────────────
 *
 * Uses an in-process transport (no real socket).  The main thread loads raw
 * Modbus TCP frames into a shared buffer; the server thread processes them
 * and writes responses back.  Both threads synchronise via s_st_mutex /
 * s_st_cond.
 * ========================================================================= */

static pthread_mutex_t s_st_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t  s_st_cond  = PTHREAD_COND_INITIALIZER;

struct SelfTestCtx {
    bool     connected;
    uint8_t  rx_buf[MAX_FRAME];
    uint16_t rx_len;
    bool     rx_ready;   /* written by main thread, cleared by st_recv        */
    uint8_t  tx_buf[MAX_FRAME];
    uint16_t tx_len;     /* written by st_send; s_st_cond is signalled        */
};

static enum MbusStatusCode st_connect(void *userdata) {
    struct SelfTestCtx *ctx = userdata;
    pthread_mutex_lock(&s_st_mutex);
    ctx->connected = true;
    pthread_mutex_unlock(&s_st_mutex);
    return MbusOk;
}
static enum MbusStatusCode st_disconnect(void *userdata) {
    struct SelfTestCtx *ctx = userdata;
    pthread_mutex_lock(&s_st_mutex);
    ctx->connected = false;
    pthread_cond_broadcast(&s_st_cond);
    pthread_mutex_unlock(&s_st_mutex);
    return MbusOk;
}
static enum MbusStatusCode st_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct SelfTestCtx *ctx = userdata;
    if (len > MAX_FRAME) return MbusErrBufferTooSmall;
    pthread_mutex_lock(&s_st_mutex);
    if (!ctx->connected) { pthread_mutex_unlock(&s_st_mutex); return MbusErrConnectionClosed; }
    memcpy(ctx->tx_buf, data, len);
    ctx->tx_len = len;
    pthread_cond_signal(&s_st_cond);
    pthread_mutex_unlock(&s_st_mutex);
    return MbusOk;
}
static enum MbusStatusCode st_recv(uint8_t *buf, uint16_t cap, uint16_t *out_len,
                                   void *userdata) {
    struct SelfTestCtx *ctx = userdata;
    pthread_mutex_lock(&s_st_mutex);
    if (!ctx->connected) { pthread_mutex_unlock(&s_st_mutex); return MbusErrConnectionClosed; }
    if (!ctx->rx_ready)  { pthread_mutex_unlock(&s_st_mutex); *out_len = 0; return MbusOk; }
    if (ctx->rx_len > cap) { pthread_mutex_unlock(&s_st_mutex); return MbusErrBufferTooSmall; }
    memcpy(buf, ctx->rx_buf, ctx->rx_len);
    *out_len      = ctx->rx_len;
    ctx->rx_ready = false;
    pthread_mutex_unlock(&s_st_mutex);
    return MbusOk;
}
static uint8_t st_is_connected(void *userdata) {
    struct SelfTestCtx *ctx = userdata;
    pthread_mutex_lock(&s_st_mutex);
    uint8_t r = ctx->connected ? 1u : 0u;
    pthread_mutex_unlock(&s_st_mutex);
    return r;
}

static void *self_test_thread_fn(void *arg) {
    struct ServerThread *st = arg;
    while (atomic_load(&st->running)) {
        mbus_tcp_server_poll(st->id);
        usleep(500);
    }
    return NULL;
}

static void set_u16_be(uint8_t *dst, uint16_t v) {
    dst[0] = (uint8_t)((v >> 8) & 0xFFu);
    dst[1] = (uint8_t)(v        & 0xFFu);
}

/* Load a Modbus TCP frame into the in-process rx buffer and reset tx_len. */
static void st_load_frame(struct SelfTestCtx *ctx, const uint8_t *frame, uint16_t len) {
    pthread_mutex_lock(&s_st_mutex);
    memcpy(ctx->rx_buf, frame, len);
    ctx->rx_len   = len;
    ctx->rx_ready = true;
    ctx->tx_len   = 0;
    pthread_mutex_unlock(&s_st_mutex);
}

/* Block until a response is available or timeout_ms elapses.
 * Returns 0 on success, 1 on timeout. */
static int st_wait_response(struct SelfTestCtx *ctx,
                            uint8_t *out_buf, uint16_t *out_len, int timeout_ms) {
    struct timespec dl;
    clock_gettime(CLOCK_REALTIME, &dl);
    dl.tv_sec  += timeout_ms / 1000;
    dl.tv_nsec += (long)(timeout_ms % 1000) * 1000000L;
    if (dl.tv_nsec >= 1000000000L) { dl.tv_sec++; dl.tv_nsec -= 1000000000L; }

    int timed_out = 0;
    pthread_mutex_lock(&s_st_mutex);
    while (ctx->tx_len == 0) {
        if (pthread_cond_timedwait(&s_st_cond, &s_st_mutex, &dl) != 0) { timed_out = 1; break; }
    }
    if (!timed_out) { *out_len = ctx->tx_len; memcpy(out_buf, ctx->tx_buf, ctx->tx_len); }
    pthread_mutex_unlock(&s_st_mutex);
    return timed_out;
}

static int run_self_test(void) {
    struct AppState           app    = { 0 };
    struct SelfTestCtx        tctx   = { 0 };
    struct MbusTransportCallbacks tr = {
        .userdata        = &tctx,
        .on_connect      = st_connect,
        .on_disconnect   = st_disconnect,
        .on_send         = st_send,
        .on_recv         = st_recv,
        .on_is_connected = st_is_connected,
    };
    struct MbusServerHandlers handlers = mbus_server_default_handlers(&app);
    struct MbusServerConfig cfg = { .slave_address = 1u, .response_timeout_ms = 1000u };

    struct ServerThread st;
    st.id = mbus_tcp_server_new(&tr, &handlers, &cfg);
    if (st.id == MBUS_INVALID_SERVER_ID) { fprintf(stderr, "failed to create server\n"); return 1; }

    enum MbusStatusCode rc = mbus_tcp_server_connect(st.id);
    if (rc != MbusOk) {
        fprintf(stderr, "connect failed: %s\n", mbus_status_str(rc));
        mbus_tcp_server_free(st.id); return 1;
    }

    atomic_store(&st.running, 1);
    if (pthread_create(&st.tid, NULL, self_test_thread_fn, &st) != 0) {
        perror("pthread_create"); mbus_tcp_server_disconnect(st.id); mbus_tcp_server_free(st.id); return 1;
    }

    uint8_t  frame[MAX_FRAME], rsp[MAX_FRAME];
    uint16_t rsp_len;

    /* FC05 — Write Single Coil 0 (pump_run = ON) */
    set_u16_be(&frame[0], 0x1001u); set_u16_be(&frame[2], 0); set_u16_be(&frame[4], 6);
    frame[6] = 1; frame[7] = 0x05;
    set_u16_be(&frame[8], 0); set_u16_be(&frame[10], 0xFF00u);
    st_load_frame(&tctx, frame, 12u);
    if (st_wait_response(&tctx, rsp, &rsp_len, 2000) || rsp_len < 12u || rsp[7] != 0x05u) {
        fprintf(stderr, "FC05 failed\n"); goto fail;
    }

    /* FC01 — Read Coil 0: should be ON */
    set_u16_be(&frame[0], 0x1002u); set_u16_be(&frame[2], 0); set_u16_be(&frame[4], 6);
    frame[6] = 1; frame[7] = 0x01;
    set_u16_be(&frame[8], 0); set_u16_be(&frame[10], 1u);
    st_load_frame(&tctx, frame, 12u);
    if (st_wait_response(&tctx, rsp, &rsp_len, 2000) || rsp_len < 10u || rsp[7] != 0x01u) {
        fprintf(stderr, "FC01 failed\n"); goto fail;
    }
    if (rsp[8] != 0x01u || (rsp[9] & 0x01u) == 0u) {
        fprintf(stderr, "FC01 coil value mismatch\n"); goto fail;
    }

    /* FC03 — Read Holding Regs 0-1 (speed_setpoint=0x1234, pressure_limit=0x5678) */
    set_u16_be(&frame[0], 0x1003u); set_u16_be(&frame[2], 0); set_u16_be(&frame[4], 6);
    frame[6] = 1; frame[7] = 0x03;
    set_u16_be(&frame[8], 0); set_u16_be(&frame[10], 2u);
    st_load_frame(&tctx, frame, 12u);
    if (st_wait_response(&tctx, rsp, &rsp_len, 2000) || rsp_len < 13u || rsp[7] != 0x03u) {
        fprintf(stderr, "FC03 failed\n"); goto fail;
    }
    if (rsp[8] != 0x04u || rsp[9] != 0x12u || rsp[10] != 0x34u ||
        rsp[11] != 0x56u || rsp[12] != 0x78u) {
        fprintf(stderr, "FC03 register mismatch\n"); goto fail;
    }

    atomic_store(&st.running, 0);
    pthread_join(st.tid, NULL);
    mbus_tcp_server_disconnect(st.id);
    mbus_tcp_server_free(st.id);
    printf("self-test: PASS (write callbacks=%u)\n", app.write_count);
    return 0;

fail:
    atomic_store(&st.running, 0);
    pthread_join(st.tid, NULL);
    mbus_tcp_server_disconnect(st.id);
    mbus_tcp_server_free(st.id);
    return 1;
}

/* =========================================================================
 * ── SERVER MODE ────────────────────────────────────────────────────────────
 *
 * Binds a real TCP listen socket on SERVER_PORT.  The server thread accepts
 * clients, drives the Modbus state machine, and reconnects after each
 * disconnect.  The main thread runs an application simulation loop, updating
 * sensor readings every second until Ctrl-C.
 * ========================================================================= */

struct TcpCtx {
    int  listen_fd;   /* bound, listening socket — never closed until shutdown */
    int  client_fd;   /* accepted client; -1 when none                         */
};

/* Accept one client using select() with a 50 ms timeout so the server thread
 * remains responsive to the shutdown flag without blocking indefinitely. */
static enum MbusStatusCode tcp_connect(void *userdata) {
    struct TcpCtx *ctx = userdata;

    fd_set rfds;
    FD_ZERO(&rfds);
    FD_SET(ctx->listen_fd, &rfds);
    struct timeval tv = { .tv_sec = 0, .tv_usec = 50000 };

    int r = select(ctx->listen_fd + 1, &rfds, NULL, NULL, &tv);
    if (r < 0) { return (errno == EINTR) ? MbusErrTimeout : MbusErrConnectionClosed; }
    if (r == 0) return MbusErrTimeout; /* no client yet, caller will retry */

    struct sockaddr_in addr;
    socklen_t addrlen = sizeof(addr);
    int fd = accept(ctx->listen_fd, (struct sockaddr *)&addr, &addrlen);
    if (fd < 0) return MbusErrConnectionClosed;

    ctx->client_fd = fd;
    printf("[tcp] client connected  %s:%d\n",
           inet_ntoa(addr.sin_addr), ntohs(addr.sin_port));
    return MbusOk;
}

static enum MbusStatusCode tcp_disconnect(void *userdata) {
    struct TcpCtx *ctx = userdata;
    if (ctx->client_fd >= 0) {
        close(ctx->client_fd);
        ctx->client_fd = -1;
        printf("[tcp] client disconnected\n");
    }
    return MbusOk;
}

/* Blocking send with retry loop. */
static enum MbusStatusCode tcp_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct TcpCtx *ctx = userdata;
    if (ctx->client_fd < 0) return MbusErrConnectionClosed;
    ssize_t sent = 0;
    while (sent < (ssize_t)len) {
        ssize_t n = send(ctx->client_fd, data + sent, (size_t)(len - sent), 0);
        if (n <= 0) return MbusErrConnectionClosed;
        sent += n;
    }
    return MbusOk;
}

/* Non-blocking receive — returns 0 bytes if nothing is available. */
static enum MbusStatusCode tcp_recv(uint8_t *buf, uint16_t cap, uint16_t *out_len,
                                    void *userdata) {
    struct TcpCtx *ctx = userdata;
    if (ctx->client_fd < 0) return MbusErrConnectionClosed;
    ssize_t n = recv(ctx->client_fd, buf, cap, MSG_DONTWAIT);
    if (n > 0)  { *out_len = (uint16_t)n; return MbusOk; }
    if (n == 0) return MbusErrConnectionClosed;
    if (errno == EAGAIN || errno == EWOULDBLOCK) { *out_len = 0; return MbusOk; }
    return MbusErrConnectionClosed;
}

static uint8_t tcp_is_connected(void *userdata) {
    struct TcpCtx *ctx = userdata;
    return (ctx->client_fd >= 0) ? 1u : 0u;
}

/* Server poll thread: accept → serve → disconnect → repeat until shutdown. */
static void *tcp_server_thread_fn(void *arg) {
    struct ServerThread *st = arg;
    while (atomic_load(&st->running)) {
        if (!mbus_tcp_server_is_connected(st->id)) {
            /* tcp_connect uses select(50ms) so this returns quickly. */
            enum MbusStatusCode rc = mbus_tcp_server_connect(st->id);
            if (rc != MbusOk) {
                usleep(10000); /* 10 ms before retry */
                continue;
            }
        }
        /* Drive the Modbus state machine.
         * tcp_recv is non-blocking so this returns in microseconds. */
        mbus_tcp_server_poll(st->id);
        usleep(500);
    }
    return NULL;
}

static int run_server(void) {
    /* Ignore SIGPIPE so a dropped client returns EPIPE from send() instead
     * of killing the process. */
    signal(SIGPIPE, SIG_IGN);

    atomic_store(&g_running, 1);
    struct sigaction sa = { .sa_handler = sig_handler };
    sigemptyset(&sa.sa_mask);
    sigaction(SIGINT,  &sa, NULL);
    sigaction(SIGTERM, &sa, NULL);

    /* ── Bind listen socket ─────────────────────────────────────────────── */
    int listen_fd = socket(AF_INET, SOCK_STREAM, 0);
    if (listen_fd < 0) { perror("socket"); return 1; }

    int opt = 1;
    setsockopt(listen_fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

    struct sockaddr_in addr = {
        .sin_family      = AF_INET,
        .sin_port        = htons(SERVER_PORT),
        .sin_addr.s_addr = INADDR_ANY,
    };
    if (bind(listen_fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("bind"); close(listen_fd); return 1;
    }
    if (listen(listen_fd, 1) < 0) {
        perror("listen"); close(listen_fd); return 1;
    }

    /* ── Application and transport state ───────────────────────────────── */
    struct AppState app  = { 0 };
    struct TcpCtx   tctx = { .listen_fd = listen_fd, .client_fd = -1 };

    struct MbusTransportCallbacks tr = {
        .userdata        = &tctx,
        .on_connect      = tcp_connect,
        .on_disconnect   = tcp_disconnect,
        .on_send         = tcp_send,
        .on_recv         = tcp_recv,
        .on_is_connected = tcp_is_connected,
    };

    /* ── Seed initial register / coil values ────────────────────────────── */
    mbus_server_set_speed_setpoint(1500u);    /* RPM  */
    mbus_server_set_pressure_limit(250u);     /* bar  */
    mbus_server_set_pressure_actual(120u);
    mbus_server_set_temperature_actual(65u);  /* °C   */
    mbus_server_set_fault_active(0u);

    /* ── Create server ──────────────────────────────────────────────────── */
    struct MbusServerHandlers handlers = mbus_server_default_handlers(&app);
    struct MbusServerConfig   cfg      = { .slave_address = 1u, .response_timeout_ms = 1000u };

    struct ServerThread st;
    st.id = mbus_tcp_server_new(&tr, &handlers, &cfg);
    if (st.id == MBUS_INVALID_SERVER_ID) {
        fprintf(stderr, "mbus_tcp_server_new failed\n"); close(listen_fd); return 1;
    }

    /* ── Start the poll/accept thread ───────────────────────────────────── */
    atomic_store(&st.running, 1);
    if (pthread_create(&st.tid, NULL, tcp_server_thread_fn, &st) != 0) {
        perror("pthread_create"); mbus_tcp_server_free(st.id); close(listen_fd); return 1;
    }

    printf("Modbus TCP server listening on 0.0.0.0:%d  (unit id 1)\n\n", SERVER_PORT);
    printf("  Coils        0x0000  pump_run         FC01 / FC05\n");
    printf("  Coils        0x0001  alarm_ack        FC01 / FC05\n");
    printf("  Holding regs 0x0000  speed_setpoint   FC03 / FC06\n");
    printf("  Holding regs 0x0001  pressure_limit   FC03 / FC06\n");
    printf("  Input regs   0x0000  pressure_actual  FC04 (read-only)\n");
    printf("  Input regs   0x0001  temperature_act  FC04 (read-only)\n");
    printf("  Discrete inp 0x0000  fault_active     FC02 (read-only)\n");
    printf("\nWaiting for client — press Ctrl-C to stop.\n\n");

    /* ── Simulation loop — update live sensor readings every second ─────── */
    uint32_t tick = 0;
    while (atomic_load(&g_running)) {
        sleep(1);
        tick++;

        uint16_t pressure = (uint16_t)(100u + (tick % 60u));
        uint16_t temp     = (uint16_t)(55u  + (tick % 40u));
        mbus_server_set_pressure_actual(pressure);
        mbus_server_set_temperature_actual(temp);

        printf("[sim] tick=%-4u  pressure=%3u bar  temp=%2u°C  "
               "pump=%-3s  writes=%u\n",
               tick, pressure, temp,
               app.pump_running ? "ON" : "OFF",
               app.write_count);
    }

    /* ── Graceful shutdown ──────────────────────────────────────────────── */
    printf("\nShutting down...\n");
    atomic_store(&st.running, 0);
    pthread_join(st.tid, NULL);                  /* waits ≤ ~50 ms              */
    if (mbus_tcp_server_is_connected(st.id))
        mbus_tcp_server_disconnect(st.id);
    mbus_tcp_server_free(st.id);
    close(listen_fd);
    printf("Stopped. Total register writes: %u\n", app.write_count);
    return 0;
}

/* =========================================================================
 * main
 * ========================================================================= */
int main(int argc, char **argv) {
    /* --self-test: run the in-process verification suite and exit.
     * Used by CTest so no network port is needed. */
    if (argc > 1 && strcmp(argv[1], "--self-test") == 0) {
        mbus_server_model_init();
        mbus_server_set_speed_setpoint(0x1234u);
        mbus_server_set_pressure_limit(0x5678u);
        mbus_server_set_pressure_actual(0x1111u);
        mbus_server_set_temperature_actual(0x2222u);
        mbus_server_set_fault_active(0u);
        return run_self_test();
    }

    /* Default: real TCP server. */
    mbus_server_model_init();
    return run_server();
}
