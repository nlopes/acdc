import assert from 'node:assert/strict'
import { test } from 'node:test'
import { verifyReport } from './check.mjs'

test('requires all pinned cases without changing the report', () => {
  const report = { pass: 13, fail: 0, errors: [] }
  const before = structuredClone(report)
  verifyReport(report)
  assert.deepEqual(report, before)
})

for (const [name, report] of [
  ['a failed case', { pass: 12, fail: 1, errors: ['mismatch'] }],
  ['the old two-failure allowance', { pass: 11, fail: 2, errors: ['listing', 'section'] }],
  ['a skipped case', { pass: 12, fail: 0, errors: [] }],
  ['a changed corpus', { pass: 14, fail: 0, errors: [] }],
  ['a failure despite the pass count', { pass: 13, fail: 1, errors: [] }],
  ['an infrastructure error', { pass: 13, fail: 0, errors: ['adapter crash'] }],
  ['a missing error report', { pass: 13, fail: 0 }],
]) {
  test(`rejects ${name}`, () => {
    assert.throws(() => verifyReport(report))
  })
}
