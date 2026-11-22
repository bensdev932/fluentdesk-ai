import { bindings, defineConfig } from 'cf/config';

export default defineConfig({
  worker: {
    name: 'fluentdesk-ai',
    entrypoint: './worker/index.ts',
    compatibilityDate: '2026-11-17',
    workersDev: false,
    observability: {
      enabled: true,
    },
    assets: {
      notFoundHandling: 'single-page-application',
      runWorkerFirst: ['/api/*'],
    },
    env: {
      AI: bindings.ai(),
      ASSETS: bindings.assets(),
    },
  },
});
