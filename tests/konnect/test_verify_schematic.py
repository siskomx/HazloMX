"""Self-checks for the independent schematic edit verifier."""
import unittest
from verify_schematic import tree, assert_only_value_changed


class VerificationTests(unittest.TestCase):
    before = b'(kicad_sch (symbol (property "Reference" "R1") (property "Value" "10k") (at 1.0 2)) (symbol (property "Reference" "R2") (property "Value" "10k")) (wire (pts (xy 0 0) (xy 1 2))))'
    after = before.replace(b'"Value" "10k"', b'"Value" "12.34k"', 1)

    def test_only_selected_value_and_numeric_spelling(self):
        assert_only_value_changed(self.before, self.after.replace(b'1.0', b'1.000'), 'R1', '12.34k')
        for altered in [self.after.replace(b'"R2"', b'"R3"'),
                        self.after.replace(b'(xy 0 0)', b'(xy 0 1)'),
                        self.after.replace(b'(at 1.0 2)', b'(at 3 2)'),
                        self.before]:
            with self.assertRaises(AssertionError):
                assert_only_value_changed(self.before, altered, 'R1', '12.34k')
        with self.assertRaises(AssertionError):
            assert_only_value_changed(self.before, self.after, 'R9', '12.34k')

    def test_malformed_document_is_refused(self):
        for data in [b'', b'(kicad_sch', b'(kicad_sch))', b'outside (kicad_sch)', b'(other)', b'(kicad_sch) (kicad_sch)']:
            with self.assertRaises(AssertionError):
                tree(data)


if __name__ == '__main__':
    unittest.main()
