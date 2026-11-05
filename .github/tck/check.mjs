import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { appendFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const revision = 'cdfada9c2768b164eadf4bc12e9f9c68e6caf68a'
const listingPath = 'tests/block/listing/multiple-lines-input.adoc'
const sectionPath = 'tests/block/section/title-body-input.adoc'

// These are still upstream failures. Only their complete observed behavior is accepted.
export function verifyKnownIncompatibilities({ report, listing, section, expectedSection }) {
  assert.equal(report.pass, 11, 'the pinned TCK must retain all 11 passing cases')
  assert.equal(report.fail, 2, 'the pinned TCK must have exactly two known failures')
  assert.equal(report.errors.length, 2, 'both known failures need upstream diagnostics')
  assert.equal(report.errors.filter((error) => error.includes('/blocks/0/metadata/caption')).length, 1)
  assert.equal(report.errors.filter((error) => error.includes(sectionPath)).length, 1)

  assert.equal(listing.status, 1, 'the listing must retain its strict-export rejection')
  assert.equal(listing.stdout, '', 'a rejected export must not write partial JSON')
  assert.deepEqual(JSON.parse(listing.stderr), {
    construct: 'listing',
    kind: 'unsupported-asg',
    location: [{ col: 1, line: 1 }, { col: 4, line: 5 }],
    message: 'this semantic property has no supported upstream mapping',
    path: '/blocks/0/metadata/caption',
    severity: 'error',
  })

  assert.equal(section.status, 0, 'the section must retain its generated identity')
  assert.equal(section.stderr, '', 'the section must not introduce diagnostics')
  const expected = structuredClone(expectedSection)
  assert.equal(Object.hasOwn(expected.blocks[0], 'id'), false, 'the upstream ID expectation changed')
  expected.blocks[0].id = '_section_title'
  assert.deepEqual(JSON.parse(section.stdout), expected, 'only the exact generated ID may differ')
}

function probe(binary, fixture) {
  const result = spawnSync(binary, ['tck'], {
    input: JSON.stringify({ contents: fixture.input, path: fixture.inputPath, type: 'block' }),
    encoding: 'utf8',
    timeout: 30_000,
  })
  if (result.error) throw result.error
  assert.equal(result.signal, null, 'the TCK adapter must exit normally')
  return result
}

function* fixtures(entries) {
  for (const entry of entries) {
    if (entry.type === 'dir') yield* fixtures(entry.entries)
    else yield entry.data
  }
}

async function main(args) {
  assert.equal(args.length, 2, 'usage: node .github/tck/check.mjs TCK_CHECKOUT ACDC_BINARY')
  const checkout = path.resolve(args[0])
  const binary = path.resolve(args[1])
  const git = (...args) => execFileSync('git', ['-C', checkout, ...args], { encoding: 'utf8' }).trim()
  const verifyCheckout = () => {
    assert.equal(git('rev-parse', 'HEAD'), revision, 'the TCK revision changed')
    assert.equal(git('status', '--porcelain', '--untracked-files=all'), '', 'the TCK checkout must be unchanged')
  }
  verifyCheckout()

  const upstream = (file) => import(pathToFileURL(path.join(checkout, 'harness/lib', file)).href)
  const [{ default: run }, { default: loadTests }] = await Promise.all([
    upstream('runner.js'), upstream('test-loader.js'),
  ])
  const testsDir = path.join(checkout, 'tests')
  const cases = [...fixtures(await loadTests(testsDir, checkout))]
  assert.equal(cases.length, 13, 'the pinned corpus must contain all 13 cases')
  const find = (name) => {
    const matches = cases.filter((fixture) => fixture.inputPath.split(path.sep).join('/') === name)
    assert.equal(matches.length, 1, `expected exactly one ${name} case`)
    return matches[0]
  }

  process.env.RUST_LOG = ''
  process.env.NO_COLOR = '1'
  // The official CLI adapter executes a shell command; keep the binary path one argument.
  const quotedBinary = "'" + binary.replaceAll("'", "'\\''") + "'"
  const report = await run({ testsDir, adapter: { cli: { command: `${quotedBinary} tck` } } })
  const listing = find(listingPath)
  const section = find(sectionPath)
  verifyKnownIncompatibilities({
    report,
    listing: probe(binary, listing),
    section: probe(binary, section),
    expectedSection: section.expected,
  })
  verifyCheckout()

  const summary = 'Official TCK: 11 passed, 2 failed; both exact known incompatibilities verified.\n' +
    'Listing caption state is rejected; the generated section ID is preserved.\n' +
    'The upstream tests, expected output, and export semantics are unchanged.\n'
  console.log(summary)
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, summary)
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error)
    process.exitCode = 1
  })
}
