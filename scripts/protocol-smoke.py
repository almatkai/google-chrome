"""Real JSONL executable tests. Every browser file and host effect is synthetic."""
import json, os, pathlib, queue, sqlite3, subprocess, sys, tempfile, threading, uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(sys.argv[1]).resolve()
checks = []

class Host:
    def __init__(self, scratch, platform):
        self.scratch = scratch
        env = dict(os.environ)
        # Process-local fixture HOME. Never inspect the real Chrome data directory.
        env['HOME'] = str(scratch / 'home')
        self.process = subprocess.Popen([str(BINARY)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, env=env)
        self.queue = queue.Queue()
        def pump():
            for line in self.process.stdout:
                self.queue.put(json.loads(line))
        threading.Thread(target=pump, daemon=True).start()
        hello = self.queue.get(timeout=15)
        assert hello['type'] == 'hello' and hello['id'] == 'com.tezbar.native.google-chrome'
        self.serial = 0
        self.calls = []
        for d in ['data', 'temp']:
            (scratch / d).mkdir(exist_ok=True)
        self.call('initialize', dict(protocol=1, extensionId=hello['id'], platform=platform,
            packageDir=str(ROOT), dataDir=str(scratch / 'data'), tempDir=str(scratch / 'temp'),
            grantedCapabilities=['clipboard.write', 'window.visibility'], migrationPaths={}, reason='test'))

    def send(self, frame):
        self.process.stdin.write((json.dumps(frame) + '\n').encode('utf-8'))
        self.process.stdin.flush()

    def call(self, method, params, error=False):
        self.serial += 1
        ident = 'h:' + str(uuid.uuid4())
        self.send(dict(type='request', id=ident, method=method, params=params))
        while True:
            frame = self.queue.get(timeout=15)
            if frame['type'] == 'request':
                # Only capture synthetic clipboard copying. No real host service runs.
                assert frame['method'] == 'host.clipboard.write', frame
                self.calls.append(frame)
                self.send(dict(type='response', id=frame['id'], result=True))
            elif frame['type'] == 'response' and frame['id'] == ident:
                if error:
                    assert 'error' in frame, frame
                    return frame['error']
                assert 'error' not in frame, frame
                return frame['result']

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=15)
        assert self.process.returncode == 0

def rpc(host, session, operation, **payload):
    return host.call('session.call', dict(sessionId=session, operation=operation, payload=payload))

def rows(view):
    return [v['props'] for v in view['root']['children'] if v['type'] == 'List.Item' and not v['props']['id'].startswith('error:')]

with tempfile.TemporaryDirectory(prefix='tezbar-chrome-fixture-') as temp:
    scratch = pathlib.Path(temp)
    base = scratch / 'home/Library/Application Support/Google/Chrome'
    profile = base / 'Default'
    profile.mkdir(parents=True)
    (base / 'Local State').write_text(json.dumps({'profile': {'info_cache': {'Default': {'name': 'Synthetic'}}}}))
    database = sqlite3.connect(profile / 'History')
    database.executescript("CREATE TABLE urls(id INTEGER,url TEXT,title TEXT,last_visit_time INTEGER);"
                          "INSERT INTO urls VALUES(1,'https://fixture.invalid/a','Alpha One',13000000000000000),"
                          "(2,'https://fixture.invalid/b','Beta Two',14000000000000000);")
    database.close()
    bookmark = {'roots': {'bar': {'type': 'folder', 'children': [dict(type='url', id='42', name='Fixture Bookmark', url='https://fixture.invalid/bookmark')]}}}
    (profile / 'Bookmarks').write_text(json.dumps(bookmark))
    original = {p: p.read_bytes() for p in base.rglob('*') if p.is_file()}
    host = Host(scratch, 'macOS')
    view = host.call('session.open', dict(sessionId='history', commandName='search-history', preferences={}, argumentValues={}))
    assert [x['title'] for x in rows(view)] == ['Beta Two', 'Alpha One']
    checks.append('history descending order')
    old_action = next(a['id'] for a in view['actions'] if a['title'] == 'Copy URL')
    view = rpc(host, 'history', 'extension:search-text-changed', searchText='alpha -beta')
    assert [x['title'] for x in rows(view)] == ['Alpha One']
    checks.append('include/exclude protocol search')
    host.call('session.call', dict(sessionId='history', operation='extension:invoke-action', payload={'actionId': old_action}), error=True)
    assert not host.calls
    checks.append('stale action rejected without effect')
    copy_action = next(a['id'] for a in view['actions'] if a['title'] == 'Copy URL')
    rpc(host, 'history', 'extension:invoke-action', actionId=copy_action)
    assert host.calls[-1]['params'] == {'text': 'https://fixture.invalid/a'}
    checks.append('exact synthetic clipboard reverse RPC')
    view = host.call('session.open', dict(sessionId='bookmarks', commandName='search-bookmarks', preferences={}, argumentValues={}))
    assert rows(view)[0]['title'] == 'Fixture Bookmark'
    checks.append('bookmark protocol view')
    host.call('session.call', dict(sessionId='bookmarks', operation='extension:invoke-action', payload={'actionId': copy_action}), error=True)
    assert len(host.calls) == 1
    checks.append('cross-session action rejected without effect')
    reset = next(a['id'] for a in view['actions'] if a['title'] == 'Reset Ranking')
    rpc(host, 'bookmarks', 'extension:invoke-action', actionId=reset)
    checks.append('bookmark reset transaction')
    for session in ['history', 'bookmarks']:
        host.call('session.dispose', dict(sessionId=session))
    host.close()
    assert all(p.read_bytes() == data for p, data in original.items())
    assert set(original) == {p for p in base.rglob('*') if p.is_file()}
    checks.append('fixture Chrome files unchanged, no added browser files')
    second = scratch / 'unsupported'; second.mkdir()
    host = Host(second, 'Windows')
    commands = json.loads(subprocess.check_output([str(BINARY), '--protocol-info']))['commands']
    assert len(commands) == 11
    for i, command in enumerate(commands):
        error = host.call('session.open', dict(sessionId=str(i), commandName=command, preferences={}, argumentValues={}), error=True)
        assert 'macOS only' in error['message'], error
    assert not host.calls
    host.close()
    checks.append('all 11 commands reject unsupported Windows platform without host effects')

report = dict(binary=str(BINARY), checks=checks, passed=len(checks),
              limitation='Synthetic history/bookmark/JSONL checks on '+sys.platform+'. No real browser automation or desktop acceptance.')
(ROOT / 'reports/protocol-smoke.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
