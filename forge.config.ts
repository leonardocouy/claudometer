import type { ForgeConfig } from '@electron-forge/shared-types';
import { MakerZIP } from '@electron-forge/maker-zip';
import { MakerDeb } from '@electron-forge/maker-deb';
import { MakerDMG } from '@electron-forge/maker-dmg';
import { VitePlugin } from '@electron-forge/plugin-vite';
import { PublisherGithub } from '@electron-forge/publisher-github';

// Import app configuration
import { config as appConfig } from './app.config';

const config: ForgeConfig = {
  packagerConfig: {
    name: appConfig.productName,
    executableName: appConfig.executableName,
    icon: './assets/tray-icon',
    appBundleId: appConfig.appBundleId,
    appCategoryType: appConfig.appCategory,
    asar: true,
    // macOS code signing (only applied when env vars are set)
    ...(process.env.APPLE_ID &&
      process.env.APPLE_PASSWORD &&
      process.env.APPLE_TEAM_ID && {
        osxSign: {
          identity: process.env.APPLE_SIGNING_IDENTITY || 'Developer ID Application',
          identityValidation: true,
        },
        osxNotarize: {
          appleId: process.env.APPLE_ID,
          appleIdPassword: process.env.APPLE_PASSWORD,
          teamId: process.env.APPLE_TEAM_ID,
        },
      }),
  },
  makers: [
    new MakerZIP({}, ['darwin', 'linux']),
    new MakerDMG({
      format: 'ULFO',
      icon: './assets/tray-icon.icns',
    }),
    new MakerDeb({
      options: {
        icon: './assets/tray-icon.png',
        maintainer: appConfig.maintainer,
        homepage: `https://github.com/${appConfig.github.owner}/${appConfig.github.repo}`,
      },
    }),
  ],
  publishers: [
    new PublisherGithub({
      repository: {
        owner: appConfig.github.owner,
        name: appConfig.github.repo,
      },
      prerelease: false,
      draft: false,
    }),
  ],
  plugins: [
    new VitePlugin({
      build: [
        {
          entry: 'src/main.ts',
          config: 'vite.main.config.ts',
        },
      ],
      renderer: [],
    }),
  ],
};

export default config;
