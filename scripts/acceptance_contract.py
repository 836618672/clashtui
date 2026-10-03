"""Fail-closed acceptance decisions shared by the runner and regression tests."""
REQUIRED_C = {*(f'C{i:02}' for i in range(1, 8)), *(f'CB{i:02}' for i in range(1, 10)), *(f'CT{i:02}' for i in range(1, 15))}


def automatic_result(stages, reports, interrupted=False):
    rows = reports.get('vm', {}).get('cases', [])
    identified = [row for row in rows if row.get('id')]
    actual = {row['id']: row for row in identified}
    missing = sorted(REQUIRED_C - actual.keys())
    failed = sorted(key for key, row in actual.items() if row.get('status') != 'passed')
    duplicates = len(actual) != len(identified)
    passed = (not interrupted and not missing and not failed and not duplicates
              and bool(stages) and all(stage.get('status') == 'passed' for stage in stages)
              and all(name in reports and reports[name].get('failed') == 0 for name in ['isolated', 'vm'])
              and bool(rows) and all(row.get('status') in ['passed', 'skipped'] for row in rows))
    return {'passed': passed, 'missing': missing, 'failed': failed, 'duplicate_ids': duplicates}


def manual_result(record, binary_sha256):
    if not isinstance(record, dict) or not binary_sha256:
        return False
    return (record.get('binary_sha256') == binary_sha256 and record.get('core_version') == '1.19.24'
            and record.get('panel_version') == 'v1.273.1' and bool(record.get('browser'))
            and bool(record.get('terminal')) and isinstance(record.get('checks'), dict)
            and all(isinstance(record['checks'].get(f'H{i:02}'), dict)
                    and record['checks'][f'H{i:02}'].get('status') == 'passed'
                    and bool(record['checks'][f'H{i:02}'].get('evidence')) for i in range(1, 7)))
