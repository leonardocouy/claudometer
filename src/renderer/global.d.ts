import type { RendererApi } from '../shared/ipc.ts';

declare global {
  interface Window {
    api: RendererApi;
  }
}

export {};

