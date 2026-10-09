//! Ties sounds to the picture events they land on: a whoosh on a cut, a pop
//! as a caption appears.

/// Something that happens on screen at a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureEvent {
    pub kind: String,
    pub frame: i64,
}

/// How far an event may sit outside a sound and still be "its" event.
/// Sounds lead into picture (a whoosh into a cut), so an event may follow
/// the sound by half a second at 30 fps, but precede it only slightly.
const AFTER: i64 = 15;
const BEFORE: i64 = 5;

/// The picture event a sound spanning `[start, end)` lands on, if any.
pub fn landing(start: i64, end: i64, events: &[PictureEvent]) -> Option<String> {
    events
        .iter()
        .filter_map(|e| {
            let gap = if e.frame < start {
                (start - e.frame <= BEFORE).then_some(start - e.frame)
            } else if e.frame >= end {
                (e.frame - end < AFTER).then_some(e.frame - end + 1)
            } else {
                Some(0)
            }?;
            Some((gap, e))
        })
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, e)| format!("{} (f{})", e.kind, e.frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: &str, frame: i64) -> PictureEvent {
        PictureEvent {
            kind: kind.into(),
            frame,
        }
    }

    #[test]
    fn a_sound_spanning_a_cut_is_on_that_cut() {
        let events = [ev("cut to shot 4", 1550), ev("caption c35 appears", 1606)];
        assert_eq!(
            landing(1527, 1557, &events),
            Some("cut to shot 4 (f1550)".to_string())
        );
    }

    #[test]
    fn a_sound_just_before_an_event_still_counts() {
        let events = [ev("caption c2 appears", 60)];
        assert_eq!(
            landing(50, 57, &events),
            Some("caption c2 appears (f60)".to_string())
        );
    }

    #[test]
    fn a_sound_far_from_any_event_lands_nowhere() {
        let events = [ev("cut to shot 2", 60)];
        assert_eq!(landing(200, 215, &events), None);
    }

    #[test]
    fn an_event_well_before_the_sound_starts_is_not_its_event() {
        // A caption at f0 and a tick heard from f14: not connected.
        let events = [ev("caption c1 appears", 0)];
        assert_eq!(landing(14, 43, &events), None);
    }

    #[test]
    fn the_nearest_event_wins() {
        let events = [ev("cut to shot 2", 100), ev("caption c4 appears", 112)];
        assert_eq!(
            landing(108, 118, &events),
            Some("caption c4 appears (f112)".to_string())
        );
    }
}
