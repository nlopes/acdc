import assert from 'node:assert/strict'
import { test } from 'node:test'
import { verifyKnownIncompatibilities } from './check.mjs'

function control() {
  const expectedSection = {
    name: 'document', type: 'block',
    blocks: [{ name: 'section', type: 'block', level: 1, title: [], blocks: [] }],
    location: [{ line: 1, col: 1 }, { line: 3, col: 9 }],
  }
  const actual = structuredClone(expectedSection)
  actual.blocks[0].id = '_section_title'
  return {
    report: {
      pass: 11, fail: 2,
      errors: ['/blocks/0/metadata/caption', 'tests/block/section/title-body-input.adoc'],
    },
    listing: {
      status: 1, stdout: '',
      stderr: JSON.stringify({
        construct: 'listing', kind: 'unsupported-asg',
        location: [{ col: 1, line: 1 }, { col: 4, line: 5 }],
        message: 'this semantic property has no supported upstream mapping',
        path: '/blocks/0/metadata/caption', severity: 'error',
      }) + '\n',
    },
    section: { status: 0, stderr: '', stdout: JSON.stringify(actual) },
    expectedSection,
  }
}

test('accepts the exact known failures without mutating upstream expectations', () => {
  const value = control()
  const before = structuredClone(value)
  verifyKnownIncompatibilities(value)
  assert.deepEqual(value, before)
})

for (const [name, change] of [
  ['an additional upstream failure', (v) => { v.report.pass--; v.report.fail++ }],
  ['an unexpected pass', (v) => { v.report.pass++; v.report.fail-- }],
  ['a skipped passing case', (v) => { v.report.pass-- }],
  ['a different failure with unchanged counts', (v) => { v.report.errors[0] = 'unrelated failure' }],
  ['a missing section failure', (v) => { v.report.errors[1] = 'unrelated failure' }],
  ['an extra harness error', (v) => { v.report.errors.push('infrastructure failure') }],
  ['partial JSON on listing rejection', (v) => { v.listing.stdout = '{}' }],
  ['a crashed listing adapter', (v) => { v.listing.status = null }],
  ['a passing listing adapter', (v) => { v.listing.status = 0 }],
  ['a crashed section adapter', (v) => { v.section.status = 1 }],
  ['a new section diagnostic', (v) => { v.section.stderr = 'warning\n' }],
  ['malformed listing diagnostics', (v) => { v.listing.stderr = 'invalid JSON' }],
  ['malformed section JSON', (v) => { v.section.stdout = 'invalid JSON' }],
]) {
  test(`rejects ${name}`, () => {
    const value = control()
    change(value)
    assert.throws(() => verifyKnownIncompatibilities(value))
  })
}

for (const [field, replacement] of [
  ['construct', 'paragraph'], ['kind', 'parse-error'], ['path', '/blocks/0/title'],
  ['severity', 'warning'], ['message', 'a different failure'],
  ['location', [{ col: 1, line: 2 }, { col: 4, line: 5 }]], ['extra', true],
]) {
  test(`rejects a changed listing ${field}`, () => {
    const value = control()
    const diagnostic = JSON.parse(value.listing.stderr)
    diagnostic[field] = replacement
    value.listing.stderr = JSON.stringify(diagnostic)
    assert.throws(() => verifyKnownIncompatibilities(value))
  })
}

for (const [name, change] of [
  ['missing ID', (v) => { delete v.blocks[0].id }],
  ['different ID', (v) => { v.blocks[0].id = '_other' }],
  ['another metadata difference', (v) => { v.blocks[0].metadata = { roles: ['new'] } }],
  ['changed title', (v) => { v.blocks[0].title.push({ name: 'text', type: 'string', value: 'Changed' }) }],
  ['changed body', (v) => { v.blocks[0].blocks.push({ name: 'paragraph', type: 'block', inlines: [] }) }],
  ['changed source location', (v) => { v.location[1].col = 8 }],
]) {
  test(`rejects section output with ${name}`, () => {
    const value = control()
    const actual = JSON.parse(value.section.stdout)
    change(actual)
    value.section.stdout = JSON.stringify(actual)
    assert.throws(() => verifyKnownIncompatibilities(value))
  })
}

test('requires review when upstream starts expecting the generated ID', () => {
  const value = control()
  value.expectedSection.blocks[0].id = '_section_title'
  assert.throws(() => verifyKnownIncompatibilities(value))
})
