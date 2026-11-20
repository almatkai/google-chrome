// Execute the actual JXA source against a synthetic Chrome dictionary. No browser.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = fs.readFileSync(require('node:path').join(__dirname, '../src/chrome.js'), 'utf8');
const calls = [];
const makeTab = id => ({id: () => id, title: () => 'Duplicate', url: () => 'https://fixture.invalid/same',
  close: () => calls.push(['close', id]), reload: () => calls.push(['reload', id])});
const first = makeTab(101), second = makeTab(102);
let current = [first, second];
const tabs = () => current;
tabs.byId = id => current.find(tab => tab.id() === id);
const win = {id: () => 1, tabs};
const windows = () => [win];
windows.whose = () => () => [win];
const chrome = {windows, activate: () => calls.push(['activate'])};
const run = input => JSON.parse(vm.runInNewContext(source, {input, Application: () => chrome}));
const captured = run({kind: 'tabs'});
assert.equal(captured[0].tabId, 101);
assert.equal(captured[1].tabId, 102);
assert.notEqual(captured[0].id, captured[1].id);
// Closing the first duplicate outside Tezbar must not redirect its old close
// action to the second tab now occupying the first position.
current = [second];
for (const kind of ['close-tab', 'reload-tab', 'focus-tab']) {
  assert.throws(() => run({...captured[0], kind}), /no longer exists/);
}
assert.deepEqual(calls, []);
// A surviving tab may move: resolve its current index and stable effect target.
run({...captured[1], kind: 'focus-tab'});
assert.equal(win.activeTabIndex, 1);
run({...captured[1], kind: 'reload-tab'});
run({...captured[1], kind: 'close-tab'});
assert.deepEqual(calls, [['activate'], ['reload', 102], ['close', 102]]);
console.log('PASS: duplicate-URL stale tab rejection and stable-ID focus/reload/close');
