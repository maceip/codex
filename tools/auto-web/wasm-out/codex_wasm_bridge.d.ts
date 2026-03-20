/* tslint:disable */
/* eslint-disable */

export function codex_cancel(cancel_json: string): void;

export function codex_deliver_callback(result_json: string): void;

export function codex_init(config_json: string): Promise<string>;

export function codex_shutdown(): Promise<void>;

export function codex_submit(request_json: string): Promise<string>;
