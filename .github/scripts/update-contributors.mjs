// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Renders the contributors table between the markers in README.md.
//
// Replaces akhilmhdh/contributors-readme-action, which resolved its target
// branch as context.ref.split('/').pop(). That yields "merge" on pull_request
// events and drops everything before the last slash on branch names like
// fix/ci-contributor, so it could only ever run against a slash-free branch.

import { readFileSync, writeFileSync } from 'node:fs'

const START = '<!-- readme: contributors -start -->'
const END = '<!-- readme: contributors -end -->'

const COLUMNS_PER_ROW = 6
const IMAGE_SIZE = 100

const README_PATH = process.env.README_PATH ?? 'README.md'
const REPOSITORY = process.env.GITHUB_REPOSITORY
const TOKEN = process.env.GITHUB_TOKEN

if (!REPOSITORY) {
  console.error('GITHUB_REPOSITORY is not set')
  process.exit(1)
}

const headers = {
  accept: 'application/vnd.github+json',
  'x-github-api-version': '2022-11-28',
  'user-agent': 'update-contributors',
}
if (TOKEN) headers.authorization = `Bearer ${TOKEN}`

const api = async (url) => {
  const response = await fetch(url, { headers })
  if (!response.ok) {
    throw new Error(`GET ${url} -> ${response.status} ${response.statusText}`)
  }
  return response.json()
}

// The contributors endpoint pages at 100 and is ordered by commit count.
const fetchContributors = async () => {
  const collected = []
  for (let page = 1; ; page++) {
    const batch = await api(
      `https://api.github.com/repos/${REPOSITORY}/contributors?per_page=100&page=${page}`,
    )
    collected.push(...batch)
    if (batch.length < 100) return collected
  }
}

// The contributors endpoint omits the display name, so each profile needs its
// own request. Falls back to the login when a profile has no name set.
const fetchDisplayName = async (login) => {
  try {
    const user = await api(`https://api.github.com/users/${login}`)
    return user.name?.trim() || login
  } catch {
    return login
  }
}

const escapeHtml = (value) =>
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')

const renderCell = ({ login, avatar_url, displayName }) =>
  [
    '\t\t\t<td align="center">',
    `\t\t\t\t<a href="https://github.com/${encodeURIComponent(login)}">`,
    `\t\t\t\t\t<img src="${escapeHtml(avatar_url)}" width="${IMAGE_SIZE};" alt="${escapeHtml(login)}"/>`,
    '\t\t\t\t\t<br />',
    `\t\t\t\t\t<sub><b>${escapeHtml(displayName)}</b></sub>`,
    '\t\t\t\t</a>',
    '\t\t\t</td>',
  ].join('\n')

const renderTable = (contributors) => {
  const rows = []
  for (let i = 0; i < contributors.length; i += COLUMNS_PER_ROW) {
    const cells = contributors.slice(i, i + COLUMNS_PER_ROW).map(renderCell)
    rows.push(['\t\t<tr>', ...cells, '\t\t</tr>'].join('\n'))
  }
  return ['<table>', '\t<tbody>', ...rows, '\t</tbody>', '</table>'].join('\n')
}

const contributors = (await fetchContributors()).filter(
  (c) => c.type !== 'Bot' && !c.login.endsWith('[bot]'),
)

if (contributors.length === 0) {
  console.error('Refusing to write an empty contributors table')
  process.exit(1)
}

const withNames = await Promise.all(
  contributors.map(async (c) => ({ ...c, displayName: await fetchDisplayName(c.login) })),
)

const readme = readFileSync(README_PATH, 'utf8')
const start = readme.indexOf(START)
const end = readme.indexOf(END)

if (start === -1 || end === -1 || end < start) {
  console.error(`Could not find the contributors markers in ${README_PATH}`)
  console.error(`Expected exactly:\n${START}\n${END}`)
  process.exit(1)
}

const updated =
  readme.slice(0, start + START.length) +
  '\n' +
  renderTable(withNames) +
  '\n' +
  readme.slice(end)

if (updated === readme) {
  console.log(`No change (${withNames.length} contributors)`)
  process.exit(0)
}

writeFileSync(README_PATH, updated)
console.log(`Updated ${README_PATH} with ${withNames.length} contributors`)
