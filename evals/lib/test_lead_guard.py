"""Protocol tests for the eval emulator; these do not supply a product guard."""

import contextlib
import io
import json
import os
from pathlib import Path
import shlex
import sys
import tempfile
import unittest
from unittest.mock import patch

import lead_guard as guard


class HookProtocolTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'project'
        (self.root / '.claude').mkdir(parents=True)
        home = Path(self.tmp.name) / 'home'
        home.mkdir()
        self.env = patch.dict(os.environ, HOME=str(home), EVALS_WORK=self.tmp.name)
        self.env.start()
        self.addCleanup(self.env.stop)
        guard.FAILURES.clear()
        self.hooks = guard.Hooks(self.root)

    def install(self, event, body, matcher=''):
        script = Path(self.tmp.name) / 'hook.py'
        script.write_text(body)
        command = shlex.join([sys.executable, str(script)])
        self.settings({'hooks': {event: [{'matcher': matcher, 'hooks': [
            {'type': 'command', 'command': command}]}]}})

    def settings(self, data):
        (self.root / '.claude/settings.json').write_text(json.dumps(data))

    def test_missing_hook_allows_and_emulator_applies_edit(self):
        self.hooks.edit('Write', 'shop/new.py', True, restore=False, content='new\n')
        self.assertEqual((self.root / 'shop/new.py').read_text(), 'new\n')
        self.assertEqual(guard.FAILURES, [])

    def test_exit_two_denies_without_applying_edit(self):
        self.install('PreToolUse', "import sys\nsys.stderr.write('shop/new.py: write a spec and /delegate it')\nsys.exit(2)\n")
        self.hooks.edit('Write', 'shop/new.py', False)
        self.assertFalse((self.root / 'shop/new.py').exists())
        self.assertEqual(guard.FAILURES, [])

    def test_json_deny_and_notebook_input(self):
        self.install('PreToolUse', '''import json, os, sys
p = json.load(sys.stdin)
assert p['tool_name'] == 'NotebookEdit'
assert p['tool_input'] == {'notebook_path': 'prices.ipynb'}
assert p['hook_event_name'] == 'PreToolUse'
assert p['cwd'] == os.getcwd() == os.environ['CLAUDE_PROJECT_DIR']
assert p['session_id'] and os.path.isfile(p['transcript_path'])
print(json.dumps({'hookSpecificOutput': {'hookEventName': 'PreToolUse',
    'permissionDecision': 'deny', 'permissionDecisionReason': 'prices.ipynb: write a spec and /delegate it'}}))
''', matcher='^(Edit|Write|NotebookEdit)$')
        self.hooks.edit('NotebookEdit', 'prices.ipynb', False)
        self.assertEqual(guard.FAILURES, [])

    def test_matcher_only_applies_to_pre_tool_use_and_all_commands_run(self):
        marker = Path(self.tmp.name) / 'calls'
        command = 'printf x >> ' + shlex.quote(str(marker))
        group = {'matcher': '^Read$', 'hooks': [
            {'type': 'command', 'command': command},
            {'type': 'command', 'command': command}]}
        self.settings({'hooks': {'PreToolUse': [group], 'Stop': [group]}})
        self.hooks.call('PreToolUse', tool='Edit', path='x.py')
        self.assertFalse(marker.exists())
        self.hooks.call('Stop')
        self.assertEqual(marker.read_text(), 'xx')
        self.assertEqual(guard.FAILURES, [])

    def test_both_stop_hold_forms_and_active_retry(self):
        for body in ("import sys\nsys.stderr.write('undo shop/cart.py and delegate')\nsys.exit(2)\n",
                     "import json\nprint(json.dumps({'decision':'block','reason':'undo shop/cart.py and delegate'}))\n"):
            self.install('Stop', body)
            result = self.hooks.call('Stop')
            self.assertTrue(result['blocked'])
            self.assertIn('shop/cart.py', result['reason'])
        self.install('Stop', "import json,sys\nassert json.load(sys.stdin)['stop_hook_active'] is True\nprint('{}')\n")
        self.assertFalse(self.hooks.call('Stop', active=True)['blocked'])
        self.assertEqual(guard.FAILURES, [])

    def test_raw_input_and_environment_are_passed_through(self):
        self.install('PreToolUse', "import os,sys\nassert sys.stdin.read() == '{'\nassert os.environ['DELEGATE_GUARD'] == 'off'\n")
        result = self.hooks.call('PreToolUse', tool='Edit', path='x.py', raw='{',
                                 env={**os.environ, 'DELEGATE_GUARD': 'off'})
        self.assertFalse(result['blocked'])
        self.assertEqual(guard.FAILURES, [])

    def test_unexpected_exit_is_a_failure_not_a_refusal(self):
        self.install('PreToolUse', 'raise SystemExit(1)\n')
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.hooks.call('PreToolUse', tool='Edit', path='x.py')
        self.assertFalse(result['blocked'])
        self.assertTrue(result['error'])
        self.assertTrue(guard.FAILURES)


if __name__ == '__main__':
    unittest.main()
