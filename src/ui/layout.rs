#[derive(Debug, Clone)]
pub struct Layout {
    pub direction: Direction,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub enum Section {
    Leaf {
        kind: SectionKind,
    },
    Node {
        direction: Direction,
        kind: SectionKind,
        sub_sections: Vec<Section>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    // TODO: pos
    pub width: u16,
    pub height: u16,
}

impl Layout {
    pub fn calc(&self, term_size: (u16, u16)) -> Vec<Rect> {
        Layout::calc_recurse(
            self.direction,
            self.sections.clone(),
            Rect {
                x: 0,
                y: 0,
                width: term_size.0,
                height: term_size.1,
            },
        )
    }

    fn calc_recurse(direction: Direction, sections: Vec<Section>, rect: Rect) -> Vec<Rect> {
        #[derive(Debug)]
        enum SizeState {
            /// a section that has been resolved to fit within the rectangle
            Resolved(u16),
            /// a section that has not yet been resolved because it needs to fill space
            WeightedFill(u8),
            /// a section that cannot be made to fit within the space provided
            DoesNotFit,
        }

        let mut rects = Vec::new();

        let (mut remaining, axis_start_pos, creation_fn) = match direction {
            Direction::Vertical => (
                rect.height,
                rect.y,
                (Box::new(|pos, size| Rect {
                    x: rect.x,
                    y: pos,
                    width: rect.width,
                    height: size,
                })) as Box<dyn Fn(u16, u16) -> Rect>,
            ),
            Direction::Horizontal => (
                rect.width,
                rect.x,
                (Box::new(|pos, size| Rect {
                    x: pos,
                    y: rect.y,
                    width: size,
                    height: rect.height,
                })) as Box<dyn Fn(u16, u16) -> Rect>,
            ),
        };

        let mut sizes = sections
            .iter()
            .map(|section| {
                let kind = match section {
                    Section::Leaf { kind } => kind,
                    Section::Node { kind, .. } => kind,
                };

                match kind {
                    SectionKind::Exact(size) if *size < remaining => {
                        remaining -= size;
                        SizeState::Resolved(*size)
                    }
                    SectionKind::Exact(_) => SizeState::DoesNotFit,
                    SectionKind::Fill(weight) => SizeState::WeightedFill(*weight),
                }
            })
            .collect::<Vec<_>>();

        let total_weight = u16::from(sizes.iter().fold(0, |acc, s| {
            acc + if let SizeState::WeightedFill(weight) = s {
                *weight
            } else {
                0
            }
        }));

        // split remaining_height into total_weight sections
        // each segment gets `base` height to start, and then the first `rem` sections each
        // get one more.
        // TODO: should this divide more evenly to prevent the first elements from getting
        // too much?

        //EXAMPLE: splitting 11 into 3 sections
        // 11 / 3 = 3
        // 11 % 3 = 2
        // gives [4, 4, 3]

        // then, to account for weight, each element that needs to be resolved will take
        // `weight` elements from this divided list.

        let base = remaining / total_weight;
        let rem = usize::from(remaining % total_weight);

        let mut split = vec![base; usize::from(total_weight)];
        split[..rem].iter_mut().for_each(|e| *e += 1);

        // fix up the remaining sizes that need to be weighted
        for size in sizes.iter_mut() {
            if let SizeState::WeightedFill(weight) = size {
                *size = SizeState::Resolved(split.drain(..usize::from(*weight)).sum());
            }
        }

        // convert the sizes into rects and recurse as needed
        let mut pos = axis_start_pos;
        for (elem, section) in sizes.into_iter().zip(sections) {
            match elem {
                SizeState::Resolved(size) => {
                    let rect = creation_fn(pos, size);
                    match section {
                        Section::Leaf { .. } => {
                            rects.push(rect);
                        }
                        Section::Node {
                            direction,
                            sub_sections,
                            ..
                        } => {
                            rects.extend(Layout::calc_recurse(direction, sub_sections, rect));
                        }
                    }
                    pos += size;
                }
                SizeState::WeightedFill(_) => {
                    unreachable!("NO WEIGHTED FILL SHOULD REMAIN");
                }
                SizeState::DoesNotFit => {
                    todo!("figure out how to do things that don't fit");
                }
            }
        }

        rects
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy)]
pub enum SectionKind {
    /// an exact number of lines, highest priority.
    Exact(u16),
    /// fill the remaining space, with a relative weight compared to all other fill sections in
    /// this sub-section. has lowest priority.
    Fill(u8),
}
