import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { appendFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const revision = 'cdfada9c2768b164eadf4bc12e9f9c68e6caf68a'
export function verifyReport(report) {
  assert.equal(report.pass, 13, 'all 13 pinned TCK cases must pass')
  assert.equal(report.fail, 0, 'no TCK failures are allowed')
  assert.deepEqual(report.errors, [], 'the TCK must not report errors')
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
  process.env.RUST_LOG = ''
  process.env.NO_COLOR = '1'
  // The official CLI adapter executes a shell command; keep the binary path one argument.
  const quotedBinary = "'" + binary.replaceAll("'", "'\\''") + "'"
  const report = await run({ testsDir, adapter: { cli: { command: `${quotedBinary} tck` } } })
  verifyReport(report)
  verifyCheckout()

  const summary = 'Official TCK: 13 passed, 0 failed.\n' +
    'The adapter omits derived caption state and unreferenced generated heading IDs.\n' +
    'The upstream tests and expected output are unchanged.\n'
  console.log(summary)
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, summary)
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error)
    process.exitCode = 1
  })
}
