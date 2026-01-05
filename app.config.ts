/**
 * Claudometer - Application Configuration
 *
 * This file contains all app-specific configuration that gets baked into the build.
 * It is committed to version control and shared across the team.
 *
 * For secrets (session keys, certificates), use environment variables or GitHub Secrets.
 */

export const config = {
  // ===========================================================================
  // APP IDENTITY
  // ===========================================================================

  /** App name shown in OS (menu bar, dock, task manager) */
  productName: 'Claudometer',

  /** Binary/executable name (lowercase, hyphens) */
  executableName: 'claudometer',

  /** macOS bundle identifier (reverse domain notation) */
  appBundleId: 'com.softaworks.claudometer',

  /** macOS app category */
  appCategory: 'public.app-category.productivity',

  // ===========================================================================
  // DATA STORAGE
  // ===========================================================================

  /** Folder name in user's app data directory (~/Library/Application Support on macOS) */
  appDataFolder: 'Claudometer',

  // ===========================================================================
  // GITHUB (for releases)
  // ===========================================================================

  github: {
    /** GitHub username or organization */
    owner: 'leonardocouy',

    /** Repository name */
    repo: 'claudometer',

    /** Set to true if repository is private (requires GH_TOKEN secret) */
    private: false,
  },

  // ===========================================================================
  // PACKAGE MAINTAINER (for Linux packages)
  // ===========================================================================

  maintainer: 'Softaworks <contact@softaworks.com>',
} as const;

/** Type for the config object */
export type AppConfig = typeof config;
