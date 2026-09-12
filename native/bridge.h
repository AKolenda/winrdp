/* SPDX-License-Identifier: AGPL-3.0-only */
#pragma once
#ifdef __cplusplus
extern "C" {
#endif
/* All calls except wr_free must execute on GTK's main thread. All returned
 * strings are owned UTF-8 JSON and must be released with wr_free. No Qt/GTK
 * objects or frame buffers cross the Rust/JavaScript boundary. */
char *wr_attach(void *vertical_box, void *webview);
char *wr_command(const char *operation, const char *json, const char *password);
char *wr_poll(void);
void wr_free(char *value);
#ifdef __cplusplus
}
#endif
