// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Design tokens mirror the Interfold / CRISP dashboard system
// (packages/interfold-dashboard): warm paper surfaces, deep warm ink text, a
// mint accent, Geist (sans) + Georgia (serif) + Geist Mono type.
const config = {
  content: ['./src/**/*.{js,jsx,ts,tsx,mdx}'],
  variant: {
    extend: {
      borderColor: ['disabled'],
      backgroundColor: ['disabled'],
      textColor: ['disabled'],
      boxShadow: ['disabled'],
      cursor: ['disabled'],
    },
  },
  theme: {
    extend: {
      fontFamily: {
        sans: ['Geist', 'ui-sans-serif', 'system-ui', '-apple-system', 'Segoe UI', 'Helvetica', 'Arial', 'sans-serif'],
        serif: ['Georgia', 'Times New Roman', 'serif'],
        mono: ['Geist Mono', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
      },
      colors: {
        ink: {
          DEFAULT: '#14201b',
          2: '#2c3833',
          3: '#5b6864',
          4: '#8b9893',
        },
        paper: {
          DEFAULT: '#f7f5ee',
          2: '#ffffff',
          3: '#f0ede4',
        },
        rule: {
          DEFAULT: '#e3e7e2',
          soft: '#eef1ec',
        },
        accent: {
          bg: '#e8faf0',
          soft: '#cdeede',
          deep: '#1f6b4a',
          ink: '#163d2c',
        },
        danger: {
          bg: '#fcebea',
          soft: '#f4c7c4',
          ink: '#b23a36',
        },
      },
      borderRadius: {
        field: '10px',
        card: '22px',
      },
      boxShadow: {
        card: '0 1px 0 rgba(20,32,27,0.02), 0 12px 32px -20px rgba(20,32,27,0.18)',
      },
      letterSpacing: {
        eyebrow: '0.08em',
      },
    },
  },
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  plugins: [require('@tailwindcss/typography')],
}
export default config
