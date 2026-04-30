#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "modbus_rs_server.h"

#define MAX_COILS 128
#define MAX_REGS 128
#define MAX_FRAME 260

struct AppState {
    bool coils[MAX_COILS];
    uint16_t holding[MAX_REGS];
    uint32_t callback_count;
};

struct TransportCtx {
    bool connected;
    uint8_t rx_buf[MAX_FRAME];
    uint16_t rx_len;
    bool rx_ready;
    uint8_t tx_buf[MAX_FRAME];
    uint16_t tx_len;
};

void mbus_pool_lock(void) {
    // Single-threaded demo: no-op lock hooks.
}

void mbus_pool_unlock(void) {
    // Single-threaded demo: no-op lock hooks.
}

void mbus_server_lock(MbusServerId id) {
    (void)id;
    // Single-threaded demo: no-op lock hooks.
}

void mbus_server_unlock(MbusServerId id) {
    (void)id;
    // Single-threaded demo: no-op lock hooks.
}

/* mbus_app_lock / mbus_app_unlock: no APP_MODEL in this hand-written demo. */
void mbus_app_lock(void)   {}
void mbus_app_unlock(void) {}

/* NOTE: mbus_client_lock/unlock not needed — `c` feature not used. */

/*
 * write-hook stubs — required because build.rs falls back to the bundled
 * example YAML (c_server_demo_yaml) which declares these four write hooks.
 * This hand-written demo does not use the generated APP_MODEL dispatcher,
 * so these stubs are never actually called; they just satisfy the linker.
 */
#include "mbus_server_app.h"
MbusHookStatus app_on_write_pump_run(void *ctx, uint16_t addr, uint8_t v)
    { (void)ctx; (void)addr; (void)v; return MBUS_HOOK_OK; }
MbusHookStatus app_on_write_alarm_ack(void *ctx, uint16_t addr, uint8_t v)
    { (void)ctx; (void)addr; (void)v; return MBUS_HOOK_OK; }
MbusHookStatus app_on_write_speed_setpoint(void *ctx, uint16_t addr, uint16_t v)
    { (void)ctx; (void)addr; (void)v; return MBUS_HOOK_OK; }
MbusHookStatus app_on_write_pressure_limit(void *ctx, uint16_t addr, uint16_t v)
    { (void)ctx; (void)addr; (void)v; return MBUS_HOOK_OK; }

static void set_u16_be(uint8_t *dst, uint16_t v) {
    dst[0] = (uint8_t)((v >> 8) & 0xFFu);
    dst[1] = (uint8_t)(v & 0xFFu);
}

static enum MbusStatusCode transport_connect(void *userdata) {
    struct TransportCtx *ctx = (struct TransportCtx *)userdata;
    ctx->connected = true;
    return MbusOk;
}

static enum MbusStatusCode transport_disconnect(void *userdata) {
    struct TransportCtx *ctx = (struct TransportCtx *)userdata;
    ctx->connected = false;
    return MbusOk;
}

static enum MbusStatusCode transport_send(const uint8_t *data, uint16_t len, void *userdata) {
    struct TransportCtx *ctx = (struct TransportCtx *)userdata;
    if (!ctx->connected) {
        return MbusErrConnectionClosed;
    }
    if (len > MAX_FRAME) {
        return MbusErrBufferTooSmall;
    }
    memcpy(ctx->tx_buf, data, len);
    ctx->tx_len = len;
    return MbusOk;
}

static enum MbusStatusCode transport_recv(uint8_t *buffer,
                                          uint16_t buffer_cap,
                                          uint16_t *out_len,
                                          void *userdata) {
    struct TransportCtx *ctx = (struct TransportCtx *)userdata;
    if (!ctx->connected) {
        return MbusErrConnectionClosed;
    }

    if (!ctx->rx_ready) {
        *out_len = 0;
        return MbusOk;
    }

    if (ctx->rx_len > buffer_cap) {
        return MbusErrBufferTooSmall;
    }

    memcpy(buffer, ctx->rx_buf, ctx->rx_len);
    *out_len = ctx->rx_len;
    ctx->rx_ready = false;
    return MbusOk;
}

static uint8_t transport_is_connected(void *userdata) {
    struct TransportCtx *ctx = (struct TransportCtx *)userdata;
    return ctx->connected ? 1u : 0u;
}

static enum MbusServerExceptionCode on_read_coils(struct MbusServerReadCoilsReq *req,
                                                  void *userdata) {
    struct AppState *app = (struct AppState *)userdata;
    uint16_t i;

    if (req == NULL || app == NULL) {
        return ServerDeviceFailure;
    }
    if ((uint32_t)req->address + (uint32_t)req->quantity > MAX_COILS) {
        return IllegalDataAddress;
    }

    memset(req->out_data, 0, req->out_data_len);
    for (i = 0; i < req->quantity; i++) {
        if (app->coils[req->address + i]) {
            req->out_data[i / 8u] |= (uint8_t)(1u << (i % 8u));
        }
    }
    req->out_byte_count = (uint8_t)((req->quantity + 7u) / 8u);
    app->callback_count += 1u;
    return Ok;
}

static enum MbusServerExceptionCode on_write_single_coil(const struct MbusServerWriteSingleCoilReq *req,
                                                          void *userdata) {
    struct AppState *app = (struct AppState *)userdata;

    if (req == NULL || app == NULL) {
        return ServerDeviceFailure;
    }
    if (req->address >= MAX_COILS) {
        return IllegalDataAddress;
    }

    app->coils[req->address] = req->value;
    app->callback_count += 1u;
    return Ok;
}

static enum MbusServerExceptionCode on_read_holding_registers(struct MbusServerReadHoldingRegistersReq *req,
                                                              void *userdata) {
    struct AppState *app = (struct AppState *)userdata;
    uint16_t i;
    uint16_t need;

    if (req == NULL || app == NULL) {
        return ServerDeviceFailure;
    }
    if ((uint32_t)req->address + (uint32_t)req->quantity > MAX_REGS) {
        return IllegalDataAddress;
    }

    need = (uint16_t)(req->quantity * 2u);
    if (need > req->out_data_len) {
        return IllegalDataValue;
    }

    for (i = 0; i < req->quantity; i++) {
        uint16_t v = app->holding[req->address + i];
        req->out_data[i * 2u] = (uint8_t)((v >> 8) & 0xFFu);
        req->out_data[i * 2u + 1u] = (uint8_t)(v & 0xFFu);
    }

    req->out_byte_count = (uint8_t)need;
    app->callback_count += 1u;
    return Ok;
}

static void load_fc05_write_single_coil(struct TransportCtx *ctx,
                                        uint16_t txn_id,
                                        uint8_t unit,
                                        uint16_t address,
                                        bool value) {
    uint16_t coil_word = value ? 0xFF00u : 0x0000u;
    uint8_t *p = ctx->rx_buf;
    set_u16_be(&p[0], txn_id);
    set_u16_be(&p[2], 0);
    set_u16_be(&p[4], 6);
    p[6] = unit;
    p[7] = 0x05;
    set_u16_be(&p[8], address);
    set_u16_be(&p[10], coil_word);
    ctx->rx_len = 12;
    ctx->rx_ready = true;
    ctx->tx_len = 0;
}

static void load_fc01_read_coils(struct TransportCtx *ctx,
                                 uint16_t txn_id,
                                 uint8_t unit,
                                 uint16_t address,
                                 uint16_t quantity) {
    uint8_t *p = ctx->rx_buf;
    set_u16_be(&p[0], txn_id);
    set_u16_be(&p[2], 0);
    set_u16_be(&p[4], 6);
    p[6] = unit;
    p[7] = 0x01;
    set_u16_be(&p[8], address);
    set_u16_be(&p[10], quantity);
    ctx->rx_len = 12;
    ctx->rx_ready = true;
    ctx->tx_len = 0;
}

static void load_fc03_read_holding(struct TransportCtx *ctx,
                                   uint16_t txn_id,
                                   uint8_t unit,
                                   uint16_t address,
                                   uint16_t quantity) {
    uint8_t *p = ctx->rx_buf;
    set_u16_be(&p[0], txn_id);
    set_u16_be(&p[2], 0);
    set_u16_be(&p[4], 6);
    p[6] = unit;
    p[7] = 0x03;
    set_u16_be(&p[8], address);
    set_u16_be(&p[10], quantity);
    ctx->rx_len = 12;
    ctx->rx_ready = true;
    ctx->tx_len = 0;
}

static int pump_until_response(MbusServerId id, struct TransportCtx *ctx) {
    int i;
    for (i = 0; i < 50; i++) {
        (void)mbus_tcp_server_poll(id);
        if (ctx->tx_len > 0) {
            return 0;
        }
        usleep(1000);
    }
    return 1;
}

int main(void) {
    struct AppState app;
    struct TransportCtx tctx;
    struct MbusTransportCallbacks transport;
    struct MbusServerHandlers handlers;
    struct MbusServerConfig cfg;
    MbusServerId id;
    enum MbusStatusCode st;

    memset(&app, 0, sizeof(app));
    memset(&tctx, 0, sizeof(tctx));
    memset(&transport, 0, sizeof(transport));
    memset(&handlers, 0, sizeof(handlers));
    memset(&cfg, 0, sizeof(cfg));

    app.holding[0] = 0x1234u;
    app.holding[1] = 0x5678u;

    transport.userdata = &tctx;
    transport.on_connect = transport_connect;
    transport.on_disconnect = transport_disconnect;
    transport.on_send = transport_send;
    transport.on_recv = transport_recv;
    transport.on_is_connected = transport_is_connected;

    handlers.userdata = &app;
    handlers.on_read_coils = on_read_coils;
    handlers.on_write_single_coil = on_write_single_coil;
    handlers.on_read_holding_registers = on_read_holding_registers;

    cfg.slave_address = 1;
    cfg.response_timeout_ms = 1000;

    id = mbus_tcp_server_new(&transport, &handlers, &cfg);
    if (id == MBUS_INVALID_SERVER_ID) {
        fprintf(stderr, "failed to create TCP server\n");
        return 1;
    }

    st = mbus_tcp_server_connect(id);
    if (st != MbusOk) {
        fprintf(stderr, "server connect failed: %s\n", mbus_status_str(st));
        mbus_tcp_server_free(id);
        return 1;
    }

    load_fc05_write_single_coil(&tctx, 0x1001u, 1u, 3u, true);
    if (pump_until_response(id, &tctx) != 0 || tctx.tx_len < 12 || tctx.tx_buf[7] != 0x05) {
        fprintf(stderr, "FC05 response validation failed\n");
        goto fail;
    }

    load_fc01_read_coils(&tctx, 0x1002u, 1u, 0u, 8u);
    if (pump_until_response(id, &tctx) != 0 || tctx.tx_len < 10 || tctx.tx_buf[7] != 0x01) {
        fprintf(stderr, "FC01 response validation failed\n");
        goto fail;
    }
    if (tctx.tx_buf[8] != 0x01u || (tctx.tx_buf[9] & 0x08u) == 0u) {
        fprintf(stderr, "FC01 coil payload mismatch\n");
        goto fail;
    }

    load_fc03_read_holding(&tctx, 0x1003u, 1u, 0u, 2u);
    if (pump_until_response(id, &tctx) != 0 || tctx.tx_len < 13 || tctx.tx_buf[7] != 0x03) {
        fprintf(stderr, "FC03 response validation failed\n");
        goto fail;
    }
    if (tctx.tx_buf[8] != 0x04u || tctx.tx_buf[9] != 0x12u || tctx.tx_buf[10] != 0x34u ||
        tctx.tx_buf[11] != 0x56u || tctx.tx_buf[12] != 0x78u) {
        fprintf(stderr, "FC03 register payload mismatch\n");
        goto fail;
    }

    st = mbus_tcp_server_disconnect(id);
    if (st != MbusOk) {
        fprintf(stderr, "server disconnect failed: %s\n", mbus_status_str(st));
        goto fail;
    }

    mbus_tcp_server_free(id);

    printf("c_server_demo: success (callbacks=%u)\n", app.callback_count);
    return 0;

fail:
    (void)mbus_tcp_server_disconnect(id);
    mbus_tcp_server_free(id);
    return 1;
}
