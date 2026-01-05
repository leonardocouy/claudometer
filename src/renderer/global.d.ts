import type { RendererApi } from '../common/ipc.ts';

declare global {
  interface Window {
    api: RendererApi;
  }
}

export {};
