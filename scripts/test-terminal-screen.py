import unittest
from terminal_screen import screen_text


class VisibleText(unittest.TestCase):
    def test_retained_cells_in_partial_redraw(self):
        value='\x1b[1;1HExported\x1b[1;1HProvid\x1b[1;8Hr node delay: 21 ms'
        self.assertEqual(screen_text(value,2,40).splitlines()[0], 'Provider node delay: 21 ms')
    def test_clear_does_not_keep_old_success(self):
        self.assertNotIn('passed', screen_text('passed\x1b[2J\x1b[1;1Hfailed',2,20))
    def test_wide_characters_and_cursor(self):
        self.assertEqual(screen_text('中A\x1b[1;3HB',2,10).splitlines()[0],'中B')
    def test_osc_and_dcs_are_not_visible(self):
        self.assertEqual(screen_text('\x1b]10;?\x07\x1bPnoise\x1b\\ok',2,10).splitlines()[0],'ok')


if __name__ == '__main__': unittest.main()
