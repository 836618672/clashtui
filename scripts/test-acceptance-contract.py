"""Acceptance must not turn absent, stale, failed or interrupted evidence green."""
import copy
import unittest
from acceptance_contract import REQUIRED_C, automatic_result, manual_result


class Decisions(unittest.TestCase):
    def setUp(self):
        self.stages = [{'name': 'vm', 'status': 'passed'}, {'name': 'restore-check', 'status': 'passed'}]
        self.reports = {'isolated': {'failed': 0}, 'vm': {'failed': 0, 'cases': [{'id': i, 'status': 'passed'} for i in sorted(REQUIRED_C)]}}
        self.record = {'binary_sha256': 'same-build', 'core_version': '1.19.24', 'panel_version': 'v1.273.1',
                       'browser': 'Chrome / macOS', 'terminal': 'Terminal / xterm',
                       'checks': {f'H{i:02}': {'status': 'passed', 'evidence': 'actual observation'} for i in range(1, 7)}}
    def decision(self):
        return automatic_result(self.stages, self.reports)['passed']
    def test_complete_evidence(self):
        self.assertTrue(self.decision()); self.assertTrue(manual_result(self.record, 'same-build'))
    def test_missing_case_and_report(self):
        self.reports['vm']['cases'].pop(); self.assertFalse(self.decision())
        del self.reports['isolated']; self.assertFalse(self.decision())
    def test_skip_required_case(self):
        self.reports['vm']['cases'][0]['status'] = 'skipped'; self.assertFalse(self.decision())
    def test_restore_failure_and_interruption(self):
        self.assertFalse(automatic_result(self.stages, self.reports, True)['passed'])
        self.stages[-1]['status'] = 'failed'; self.assertFalse(self.decision())
    def test_failed_case_with_wrong_summary(self):
        self.reports['vm']['cases'].append({'name': 'network', 'status': 'failed'}); self.assertFalse(self.decision())
    def test_duplicate_case_cannot_hide_failure(self):
        self.reports['vm']['cases'].append(copy.deepcopy(self.reports['vm']['cases'][0])); self.assertFalse(self.decision())
    def test_stale_binary_or_wrong_version(self):
        self.assertFalse(manual_result(self.record, 'changed-build'))
        self.record['core_version'] = 'other'; self.assertFalse(manual_result(self.record, 'same-build'))
    def test_human_record_requires_every_observation(self):
        for change in ['missing', 'failed', 'no-evidence']:
            record = copy.deepcopy(self.record)
            if change == 'missing': record['checks'].pop('H06')
            elif change == 'failed': record['checks']['H06']['status'] = 'failed'
            else: record['checks']['H06']['evidence'] = ''
            self.assertFalse(manual_result(record, 'same-build'))


if __name__ == '__main__': unittest.main()
