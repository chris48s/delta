use crate::edits::line_pair::LinePair;

/*
  Consider minus line m and paired plus line p, respectively.  The following cases exist:

  1. Whitespace deleted at line beginning.
     => The deleted section is highlighted in m; p is unstyled.

  2. Whitespace inserted at line beginning.
     => The inserted section is highlighted in p; m is unstyled.

  3. An internal section of the line containing a non-whitespace character has been deleted.
     => The deleted section is highlighted in m; p is unstyled.

  4. An internal section of the line containing a non-whitespace character has been changed.
     => The original section is highlighted in m; the replacement is highlighted in p.

  5. An internal section of the line containing a non-whitespace character has been inserted.
     => The inserted section is highlighted in p; m is unstyled.

  Note that whitespace can be neither deleted nor inserted at the end of the line: the line by
  definition has no trailing whitespace.
*/

type AnnotatedLine<'a, EditOperation> = Vec<(EditOperation, &'a str)>;

/// Infer the edit operations responsible for the differences between a collection of old and new
/// lines. Return the input minus and plus lines, in annotated form.
pub fn infer_edits<'a, EditOperation>(
    minus_lines: &'a Vec<String>,
    plus_lines: &'a Vec<String>,
    non_deletion: EditOperation,
    deletion: EditOperation,
    non_insertion: EditOperation,
    insertion: EditOperation,
    distance_threshold: f64,
) -> (
    Vec<AnnotatedLine<'a, EditOperation>>,
    Vec<AnnotatedLine<'a, EditOperation>>,
)
where
    EditOperation: Copy,
{
    let mut annotated_minus_lines = Vec::<AnnotatedLine<EditOperation>>::new();
    let mut annotated_plus_lines = Vec::<AnnotatedLine<EditOperation>>::new();

    let mut emitted = 0; // plus lines emitted so far

    'minus_lines_loop: for minus_line in minus_lines {
        let mut considered = 0; // plus lines considered so far as match for minus_line
        for plus_line in &plus_lines[emitted..] {
            let line_pair = LinePair::new(minus_line, plus_line);
            if line_pair.distance < distance_threshold {
                // minus_line and plus_line are inferred to be a homologous pair.

                // Emit as unpaired the plus lines already considered and rejected
                for plus_line in &plus_lines[emitted..(emitted + considered)] {
                    annotated_plus_lines.push(vec![(non_insertion, plus_line)]);
                }
                emitted += considered;

                // Emit the homologous pair.
                let (minus_edit, plus_edit) = (line_pair.minus_edit, line_pair.plus_edit);
                annotated_minus_lines.push(vec![
                    (non_deletion, &minus_line[0..minus_edit.start]),
                    (deletion, &minus_line[minus_edit.start..minus_edit.end]),
                    (non_deletion, &minus_line[minus_edit.end..]),
                ]);
                annotated_plus_lines.push(vec![
                    (non_insertion, &plus_line[0..plus_edit.start]),
                    (insertion, &plus_line[plus_edit.start..plus_edit.end]),
                    (non_insertion, &plus_line[plus_edit.end..]),
                ]);
                emitted += 1;

                // Move on to the next minus line.
                continue 'minus_lines_loop;
            } else {
                considered += 1;
            }
        }
        // No homolog was found for minus i; emit as unpaired.
        annotated_minus_lines.push(vec![(non_deletion, minus_line)]);
    }
    // Emit any remaining plus lines
    for plus_line in &plus_lines[emitted..] {
        annotated_plus_lines.push(vec![(non_insertion, plus_line)]);
    }

    (annotated_minus_lines, annotated_plus_lines)
}

mod line_pair {
    use std::cmp::{max, min};

    use itertools::Itertools;
    use unicode_segmentation::UnicodeSegmentation;

    /// A pair of right-trimmed strings.
    pub struct LinePair<'a> {
        pub minus_line: &'a str,
        pub plus_line: &'a str,
        pub minus_edit: Edit,
        pub plus_edit: Edit,
        pub distance: f64,
    }

    #[derive(Debug)]
    pub struct Edit {
        pub start: usize,
        pub end: usize,
        string_length: usize,
    }

    impl Edit {
        // TODO: exclude leading whitespace in this calculation
        fn distance(&self) -> f64 {
            (self.end - self.start) as f64 / self.string_length as f64
        }
    }

    impl<'a> LinePair<'a> {
        pub fn new(s0: &'a str, s1: &'a str) -> Self {
            let (g0, g1) = (s0.grapheme_indices(true), s1.grapheme_indices(true));
            let common_prefix_length = LinePair::common_prefix_length(g0, g1);
            // TODO: Don't compute grapheme segmentation twice?
            let (g0, g1) = (s0.grapheme_indices(true), s1.grapheme_indices(true));
            let (common_suffix_length, trailing_whitespace) = LinePair::suffix_data(g0, g1);
            let lengths = [
                s0.len() - trailing_whitespace[0],
                s1.len() - trailing_whitespace[1],
            ];

            // We require that (right-trimmed length) >= (common prefix length). Consider:
            // minus = "a    "
            // plus  = "a b  "
            // Here, the right-trimmed length of minus is 1, yet the common prefix length is 2. We
            // resolve this by taking the following maxima:
            let minus_length = max(lengths[0], common_prefix_length);
            let plus_length = max(lengths[1], common_prefix_length);

            // Work backwards from the end of the strings. The end of the change region is equal to
            // the start of their common suffix. To find the start of the change region, start with
            // the end of their common prefix, and then move leftwards until it is before the start
            // of the common suffix in both strings.
            let minus_change_end = minus_length - common_suffix_length;
            let plus_change_end = plus_length - common_suffix_length;
            let change_begin = min(common_prefix_length, min(minus_change_end, plus_change_end));

            let minus_edit = Edit {
                start: change_begin,
                end: minus_change_end,
                string_length: minus_length,
            };
            let plus_edit = Edit {
                start: change_begin,
                end: plus_change_end,
                string_length: plus_length,
            };
            let distance = minus_edit.distance() + plus_edit.distance();
            LinePair {
                minus_line: s0,
                plus_line: s1,
                minus_edit,
                plus_edit,
                distance,
            }
        }

        #[allow(dead_code)]
        pub fn format(&self) -> String {
            format!(
                "LinePair\n \
                 \t{} {} {}\n \
                 \t{} {} {}\n \
                 \t{}",
                self.minus_line.trim_end(),
                self.minus_edit.start,
                self.minus_edit.end,
                self.plus_line.trim_end(),
                self.plus_edit.start,
                self.plus_edit.end,
                self.distance
            )
        }

        /// Align the two strings at their left ends and consider only the bytes up to the length of
        /// the shorter string. Return the byte offset of the first differing grapheme cluster, or
        /// the byte length of shorter string if they do not differ.
        fn common_prefix_length<'b, I>(s0: I, s1: I) -> usize
        where
            I: Iterator<Item = (usize, &'b str)>,
            I: Itertools,
        {
            s0.zip(s1)
                .peekable()
                .peeking_take_while(|((_, c0), (_, c1))| c0 == c1)
                .fold(0, |offset, ((_, c0), (_, _))| offset + c0.len())
        }

        /// Trim trailing whitespace and align the two strings at their right ends. Fix the origin
        /// at their right ends and, looking left, consider only the bytes up to the length of the
        /// shorter string. Return the byte offset of the first differing grapheme cluster, or the
        /// byte length of the shorter string if they do not differ. Also return the number of bytes
        /// of whitespace trimmed from each string.
        fn suffix_data<'b, I>(s0: I, s1: I) -> (usize, [usize; 2])
        where
            I: DoubleEndedIterator<Item = (usize, &'b str)>,
            I: Itertools,
        {
            let mut s0 = s0.rev().peekable();
            let mut s1 = s1.rev().peekable();

            let is_whitespace = |(_, c): &(usize, &str)| *c == " " || *c == "\n";
            let n0 = (&mut s0).peeking_take_while(is_whitespace).count();
            let n1 = (&mut s1).peeking_take_while(is_whitespace).count();

            (LinePair::common_prefix_length(s0, s1), [n0, n1])
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn common_prefix_length(s1: &str, s2: &str) -> usize {
            super::LinePair::common_prefix_length(
                s1.grapheme_indices(true),
                s2.grapheme_indices(true),
            )
        }

        fn common_suffix_length(s1: &str, s2: &str) -> usize {
            super::LinePair::suffix_data(s1.grapheme_indices(true), s2.grapheme_indices(true)).0
        }

        #[test]
        fn test_common_prefix_length() {
            assert_eq!(common_prefix_length("", ""), 0);
            assert_eq!(common_prefix_length("", "a"), 0);
            assert_eq!(common_prefix_length("a", ""), 0);
            assert_eq!(common_prefix_length("a", "b"), 0);
            assert_eq!(common_prefix_length("a", "a"), 1);
            assert_eq!(common_prefix_length("a", "ab"), 1);
            assert_eq!(common_prefix_length("ab", "a"), 1);
            assert_eq!(common_prefix_length("ab", "aba"), 2);
            assert_eq!(common_prefix_length("aba", "ab"), 2);
        }

        #[test]
        fn test_common_prefix_length_with_leading_whitespace() {
            assert_eq!(common_prefix_length(" ", ""), 0);
            assert_eq!(common_prefix_length(" ", " "), 1);
            assert_eq!(common_prefix_length(" a", " a"), 2);
            assert_eq!(common_prefix_length(" a", "a"), 0);
        }

        #[test]
        fn test_common_suffix_length() {
            assert_eq!(common_suffix_length("", ""), 0);
            assert_eq!(common_suffix_length("", "a"), 0);
            assert_eq!(common_suffix_length("a", ""), 0);
            assert_eq!(common_suffix_length("a", "b"), 0);
            assert_eq!(common_suffix_length("a", "a"), 1);
            assert_eq!(common_suffix_length("a", "ab"), 0);
            assert_eq!(common_suffix_length("ab", "a"), 0);
            assert_eq!(common_suffix_length("ab", "b"), 1);
            assert_eq!(common_suffix_length("ab", "aab"), 2);
            assert_eq!(common_suffix_length("aba", "ba"), 2);
        }

        #[test]
        fn test_common_suffix_length_with_trailing_whitespace() {
            assert_eq!(common_suffix_length("", "  "), 0);
            assert_eq!(common_suffix_length("  ", "a"), 0);
            assert_eq!(common_suffix_length("a  ", ""), 0);
            assert_eq!(common_suffix_length("a", "b  "), 0);
            assert_eq!(common_suffix_length("a", "a  "), 1);
            assert_eq!(common_suffix_length("a  ", "ab  "), 0);
            assert_eq!(common_suffix_length("ab", "a  "), 0);
            assert_eq!(common_suffix_length("ab  ", "b "), 1);
            assert_eq!(common_suffix_length("ab ", "aab  "), 2);
            assert_eq!(common_suffix_length("aba ", "ba"), 2);
        }

        #[test]
        fn test_common_suffix_length_with_trailing_whitespace_nonascii() {
            assert_eq!(common_suffix_length("  ", "á"), 0);
            assert_eq!(common_suffix_length("á  ", ""), 0);
            assert_eq!(common_suffix_length("á", "b  "), 0);
            assert_eq!(common_suffix_length("á", "á  "), "á".len());
            assert_eq!(common_suffix_length("a  ", "áb  "), 0);
            assert_eq!(common_suffix_length("ab", "á  "), 0);
            assert_eq!(common_suffix_length("áb  ", "b "), 1);
            assert_eq!(common_suffix_length("áb ", "aáb  "), 1 + "á".len());
            assert_eq!(common_suffix_length("abá ", "bá"), 1 + "á".len());
            assert_eq!(
                common_suffix_length("áaáabá ", "ááabá   "),
                2 + 2 * "á".len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum EditOperation {
        MinusNoop,
        PlusNoop,
        Deletion,
        Insertion,
    }

    type Annotation<'a> = (EditOperation, &'a str);
    type AnnotatedLine<'a> = Vec<Annotation<'a>>;
    type AnnotatedLines<'a> = Vec<AnnotatedLine<'a>>;
    type Edits<'a> = (AnnotatedLines<'a>, AnnotatedLines<'a>);

    use EditOperation::*;

    const DISTANCE_MAX: f64 = 2.0;

    #[test]
    fn test_infer_edits_1() {
        assert_paired_edits(
            vec!["aaa\n"],
            vec!["aba\n"],
            (
                vec![vec![(MinusNoop, "a"), (Deletion, "a"), (MinusNoop, "a\n")]],
                vec![vec![(PlusNoop, "a"), (Insertion, "b"), (PlusNoop, "a\n")]],
            ),
        )
    }

    #[test]
    fn test_infer_edits_2() {
        assert_paired_edits(
            vec!["áaa\n"],
            vec!["ááb\n"],
            (
                vec![vec![(MinusNoop, "á"), (Deletion, "aa"), (MinusNoop, "\n")]],
                vec![vec![(PlusNoop, "á"), (Insertion, "áb"), (PlusNoop, "\n")]],
            ),
        )
    }

    #[test]
    fn test_infer_edits_3() {
        assert_paired_edits(
            vec!["d.iteritems()\n"],
            vec!["d.items()\n"],
            (
                vec![vec![
                    (MinusNoop, "d."),
                    (Deletion, "iter"),
                    (MinusNoop, "items()\n"),
                ]],
                vec![vec![
                    (PlusNoop, "d."),
                    (Insertion, ""),
                    (PlusNoop, "items()\n"),
                ]],
            ),
        )
    }

    #[test]
    fn test_infer_edits_4() {
        assert_edits(
            vec!["áaaáaaáaa\n", "áábáábááb\n"],
            vec!["áábáácááb\n"],
            (
                vec![
                    vec![(MinusNoop, "áaaáaaáaa\n")],
                    vec![
                        (MinusNoop, "áábáá"),
                        (Deletion, "b"),
                        (MinusNoop, "ááb\n"),
                    ],
                ],
                vec![vec![
                    (PlusNoop, "áábáá"),
                    (Insertion, "c"),
                    (PlusNoop, "ááb\n"),
                ]],
            ),
            0.66,
        )
    }

    #[test]
    fn test_infer_edits_5() {
        assert_edits(
            vec!["aaaaaaaa\n", "bbbbbbbb\n", "cccccccc\n"],
            vec!["bbbb!bbb\n", "dddddddd\n", "cccc!ccc\n"],
            (
                vec![
                    vec![(MinusNoop, "aaaaaaaa\n")],
                    vec![(MinusNoop, "bbbb"), (Deletion, "b"), (MinusNoop, "bbb\n")],
                    vec![(MinusNoop, "cccc"), (Deletion, "c"), (MinusNoop, "ccc\n")],
                ],
                vec![
                    vec![(PlusNoop, "bbbb"), (Insertion, "!"), (PlusNoop, "bbb\n")],
                    vec![(PlusNoop, "dddddddd\n")],
                    vec![(PlusNoop, "cccc"), (Insertion, "!"), (PlusNoop, "ccc\n")],
                ],
            ),
            0.66,
        )
    }

    fn assert_edits(
        minus_lines: Vec<&str>,
        plus_lines: Vec<&str>,
        expected_edits: Edits,
        distance_threshold: f64,
    ) {
        let minus_lines = minus_lines
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let plus_lines = plus_lines
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let actual_edits = infer_edits(
            &minus_lines,
            &plus_lines,
            MinusNoop,
            Deletion,
            PlusNoop,
            Insertion,
            distance_threshold,
        );
        assert_eq!(actual_edits, expected_edits);
    }

    // Assertions for a single pair of lines, considered as a homologous pair. We set
    // distance_threshold = DISTANCE_MAX in order that the pair will be inferred to be homologous.
    fn assert_paired_edits(minus_lines: Vec<&str>, plus_lines: Vec<&str>, expected_edits: Edits) {
        assert_consistent_pairs(&expected_edits);
        assert_edits(minus_lines, plus_lines, expected_edits, DISTANCE_MAX);
    }

    fn assert_consistent_pairs(edits: &Edits) {
        let (minus_annotated_lines, plus_annotated_lines) = edits;

        for (minus_annotated_line, plus_annotated_line) in
            minus_annotated_lines.iter().zip(plus_annotated_lines)
        {
            let (minus_total, minus_delta) = summarize_annotated_line(minus_annotated_line);
            let (plus_total, plus_delta) = summarize_annotated_line(plus_annotated_line);
            assert_eq!(
                minus_total - minus_delta,
                plus_total - plus_delta,
                "\nInconsistent edits:\n \
                 {:?}\n \
                 \tminus_total - minus_delta = {} - {} = {}\n \
                 {:?}\n \
                 \tplus_total  - plus_delta  = {} - {} = {}\n",
                minus_annotated_line,
                minus_total,
                minus_delta,
                minus_total - minus_delta,
                plus_annotated_line,
                plus_total,
                plus_delta,
                plus_total - plus_delta
            );
        }
    }

    fn summarize_annotated_line(sections: &AnnotatedLine) -> (usize, usize) {
        let mut total = 0;
        let mut delta = 0;
        for (edit, s) in sections {
            let length = s.graphemes(true).count();
            total += length;
            if is_edit(edit) {
                delta += length;
            }
        }
        (total, delta)
    }

    // For debugging test failures:

    #[allow(dead_code)]
    fn compare_annotated_lines(actual: Edits, expected: Edits) {
        let (minus, plus) = actual;
        println!("\n\nactual minus:");
        print_annotated_lines(minus);
        println!("\nactual plus:");
        print_annotated_lines(plus);

        let (minus, plus) = expected;
        println!("\n\nexpected minus:");
        print_annotated_lines(minus);
        println!("\nexpected plus:");
        print_annotated_lines(plus);
    }

    #[allow(dead_code)]
    fn print_annotated_lines(annotated_lines: AnnotatedLines) {
        for annotated_line in annotated_lines {
            print_annotated_line(annotated_line);
        }
    }

    #[allow(dead_code)]
    fn print_annotated_line(annotated_line: AnnotatedLine) {
        for (edit, s) in annotated_line {
            print!("({} {}), ", fmt_edit(edit), s);
        }
        print!("\n");
    }

    #[allow(dead_code)]
    fn fmt_edit(edit: EditOperation) -> &'static str {
        match edit {
            MinusNoop => "MinusNoop",
            Deletion => "Deletion",
            PlusNoop => "PlusNoop",
            Insertion => "Insertion",
        }
    }

    fn is_edit(edit: &EditOperation) -> bool {
        *edit == Deletion || *edit == Insertion
    }

}