use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr;

/// MIT License
///
/// Copyright © 2021 Tom de Bruijn
///
/// Permission is hereby granted, free of charge, to any person obtaining a copy of
/// this software and associated documentation files (the “Software”), to deal in
/// the Software without restriction, including without limitation the rights to
/// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
/// of the Software, and to permit persons to whom the Software is furnished to do
/// so, subject to the following conditions:
///
/// The above copyright notice and this permission notice shall be included in all
/// copies or substantial portions of the Software.
///
/// THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
/// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
/// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
/// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
/// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
/// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
/// SOFTWARE.
///
/// copied from <https://github.com/lintje/lintje/blob/2e958f4533bcaeb1293ecdb64bff679dc629e98f/src/utils.rs#L30>

const ZERO_WIDTH_JOINER: &str = "\u{200d}";
const VARIATION_SELECTOR_16: &str = "\u{fe0f}";
const SKIN_TONES: [&str; 5] = [
    "\u{1f3fb}", // Light Skin Tone
    "\u{1f3fc}", // Medium-Light Skin Tone
    "\u{1f3fd}", // Medium Skin Tone
    "\u{1f3fe}", // Medium-Dark Skin Tone
    "\u{1f3ff}", // Dark Skin Tone
];

// Return String display width as rendered in a monospace font according to the Unicode
// specification.
//
// This may return some odd results at times where some symbols are counted as more character width
// than they actually are.
//
// This function has exceptions for skin tones and other emoji modifiers to determine a more
// accurate display with.
pub fn display_width(string: &str) -> usize {
    // String expressed as a vec of Unicode characters. Characters with accents and emoji may
    // be multiple characters combined.
    let unicode_chars = string.graphemes(true);
    let mut width = 0;
    for c in unicode_chars {
        width += display_width_char(c);
    }
    width
}

/// Calculate the render width of a single Unicode character. Unicode characters may consist of
/// multiple String characters, which is why the function argument takes a string.
fn display_width_char(string: &str) -> usize {
    // Characters that are used as modifiers on emoji. By themselves they have no width.
    if string == ZERO_WIDTH_JOINER || string == VARIATION_SELECTOR_16 {
        return 0;
    }
    // Emoji that are representations of combined emoji. They are normally calculated as the
    // combined width of the emoji, rather than the actual display width. This check fixes that and
    // returns a width of 2 instead.
    if string.contains(ZERO_WIDTH_JOINER) {
        return 2;
    }
    // Any character with a skin tone is most likely an emoji.
    // Normally it would be counted as as four or more characters, but these emoji should be
    // rendered as having a width of two.
    for skin_tone in SKIN_TONES {
        if string.contains(skin_tone) {
            return 2;
        }
    }

    match string {
        "\t" => {
            // unicode-width returns 0 for tab width, which is not how it's rendered.
            // I choose 4 columns as that's what most applications render a tab as.
            4
        }
        _ => UnicodeWidthStr::width(string),
    }
}
