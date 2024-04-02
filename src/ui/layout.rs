#[derive(Debug, Clone)]
pub struct Layout {
    pub direction: Direction,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub direction: Direction,
    pub kind: SectionKind,
    pub sub_sections: Vec<Section>,
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

        let section_kinds = sections.iter().map(|s| s.kind).collect::<Vec<_>>();
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

        let mut sizes = section_kinds
            .iter()
            .map(|kind| match kind {
                SectionKind::Exact(size) if *size < remaining => {
                    remaining -= size;
                    SizeState::Resolved(*size)
                }
                SectionKind::Exact(_) => SizeState::DoesNotFit,
                SectionKind::Fill(weight) => SizeState::WeightedFill(*weight),
            })
            .collect::<Vec<_>>();
        dbg!(&sizes, remaining);
        let total_weight = u16::from(sizes.iter().fold(0, |acc, s| {
            acc + if let SizeState::WeightedFill(weight) = s {
                *weight
            } else {
                0
            }
        }));
        dbg!(total_weight);

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
        dbg!(&split);

        for size in sizes.iter_mut() {
            if let SizeState::WeightedFill(weight) = size {
                *size = SizeState::Resolved(split.drain(..usize::from(*weight)).sum());
            }
        }
        dbg!(&sizes);

        let mut pos = axis_start_pos;
        for (elem, section) in sizes.into_iter().zip(sections) {
            dbg!(&elem, pos);
            match elem {
                SizeState::Resolved(size) => {
                    let rect = creation_fn(pos, size);
                    if section.sub_sections.len() > 0 {
                        rects.extend(Layout::calc_recurse(
                            section.direction,
                            section.sub_sections,
                            rect,
                        ));
                    } else {
                        rects.push(rect);
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
