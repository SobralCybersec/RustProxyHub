const pluginVitest = require('@vitest/eslint-plugin')
const skipFormatting = require('@vue/eslint-config-prettier/skip-formatting')
const vueTsEslintConfig = require('@vue/eslint-config-typescript')
const security = require('eslint-plugin-security')
const pluginVue = require('eslint-plugin-vue')

/** @type {import('eslint').Linter.Config[]} */
module.exports = [
  {
    name: 'app/files-to-lint',
    files: ['**/*.{ts,mts,tsx,vue}'],
  },

  {
    name: 'app/files-to-ignore',
    ignores: ['**/dist/**', '**/dist-ssr/**', '**/coverage/**', '**/test-results/**', '*.config.*', 'src/components/effects/**', 'src-tauri/**', 'scripts/**', '*.cjs', '**/*.mjs', '**/__global-api-script.js'],
  },

  {
    name: 'app/rules',
    rules: {
      'no-var': 'error',
      'no-console': process.env.NODE_ENV === 'production' ? 'warn' : 'off',
      'no-debugger': process.env.NODE_ENV === 'production' ? 'warn' : 'off',
      'comma-dangle': ['error', 'only-multiline'],
      'id-length': [2, { exceptions: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'm', 'n', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '_'] }],
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          args: 'all',
          argsIgnorePattern: '^_',
          caughtErrors: 'all',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          ignoreRestSiblings: true,
        },
      ],
    },
  },

  ...pluginVue.configs['flat/recommended'],
  ...vueTsEslintConfig.default(),

  {
    ...pluginVitest.configs.recommended,
    files: ['tests/unit/**/*'],
  },

  skipFormatting,

  security.configs.recommended,
  {
    name: 'app/security-overrides',
    rules: {
      'security/detect-object-injection': 'off',
    },
  },
]
