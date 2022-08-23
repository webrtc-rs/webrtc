use std::fmt::Debug;

use derive_builder::Builder;

use crate::track::{
    constraint::{fitness::Fitness, NonNumeric, Numeric},
    setting::video as setting,
};

use super::Merge;

pub type Width = Numeric<setting::Width>;
pub type Height = Numeric<setting::Height>;
pub type AspectRatio = Numeric<setting::AspectRatio>;
pub type FrameRate = Numeric<setting::FrameRate>;
pub type FacingMode = NonNumeric<setting::FacingMode>;
pub type ResizeMode = NonNumeric<setting::ResizeMode>;

/// A video's constraints
#[derive(PartialEq, Default, Clone, Builder)]
pub struct Video {
    #[builder(default, setter(into, strip_option))]
    pub width: Option<Width>,
    #[builder(default, setter(into, strip_option))]
    pub height: Option<Height>,
    #[builder(default, setter(into, strip_option))]
    pub aspect_ratio: Option<AspectRatio>,
    #[builder(default, setter(into, strip_option))]
    pub frame_rate: Option<FrameRate>,
    #[builder(default, setter(into, strip_option))]
    pub facing_mode: Option<FacingMode>,
    #[builder(default, setter(into, strip_option))]
    pub resize_mode: Option<ResizeMode>,
}

impl Video {
    pub fn builder() -> VideoBuilder {
        Default::default()
    }

    pub fn new(
        width: Option<Width>,
        height: Option<Height>,
        aspect_ratio: Option<AspectRatio>,
        frame_rate: Option<FrameRate>,
        facing_mode: Option<FacingMode>,
        resize_mode: Option<ResizeMode>,
    ) -> Self {
        Self {
            width,
            height,
            aspect_ratio,
            frame_rate,
            facing_mode,
            resize_mode,
        }
    }
}

impl Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("Video");

        if let Some(width) = &self.width {
            builder.field("width", &width);
        }
        if let Some(height) = &self.height {
            builder.field("height", &height);
        }
        if let Some(aspect_ratio) = &self.aspect_ratio {
            builder.field("aspect_ratio", &aspect_ratio);
        }
        if let Some(frame_rate) = &self.frame_rate {
            builder.field("frame_rate", &frame_rate);
        }
        if let Some(facing_mode) = &self.facing_mode {
            builder.field("facing_mode", &facing_mode);
        }
        if let Some(resize_mode) = &self.resize_mode {
            builder.field("resize_mode", &resize_mode);
        }

        builder.finish()
    }
}

impl Merge for Video {
    fn merge(&mut self, other: &Self) {
        if self.width.is_none() {
            self.width = other.width.clone();
        }
        if self.height.is_none() {
            self.height = other.height.clone();
        }
        if self.aspect_ratio.is_none() {
            self.aspect_ratio = other.aspect_ratio.clone();
        }
        if self.frame_rate.is_none() {
            self.frame_rate = other.frame_rate.clone();
        }
        if self.facing_mode.is_none() {
            self.facing_mode = other.facing_mode.clone();
        }
        if self.resize_mode.is_none() {
            self.resize_mode = other.resize_mode.clone();
        }
    }
}

impl Video {
    fn fitness_distance(&self, settings: Option<&setting::Video>) -> f64 {
        // TODO(regexident): replace with `let_else` once stabilized:
        // Tracking issue: https://github.com/rust-lang/rust/issues/87335
        let settings = match settings {
            Some(settings) => settings,
            None => {
                return 0.0;
            }
        };

        let mut fitness: f64 = 0.0;

        // TODO(regexident): Ignore unsupported constraints.

        if let Some(width) = &self.width {
            fitness += width.fitness_distance(settings.width.as_ref());
        }
        if let Some(height) = &self.height {
            fitness += height.fitness_distance(settings.height.as_ref());
        }
        if let Some(aspect_ratio) = &self.aspect_ratio {
            fitness += aspect_ratio.fitness_distance(settings.aspect_ratio.as_ref());
        }
        if let Some(frame_rate) = &self.frame_rate {
            fitness += frame_rate.fitness_distance(settings.frame_rate.as_ref());
        }
        if let Some(facing_mode) = &self.facing_mode {
            fitness += facing_mode.fitness_distance(settings.facing_mode.as_ref());
        }
        if let Some(resize_mode) = &self.resize_mode {
            fitness += resize_mode.fitness_distance(settings.resize_mode.as_ref());
        }

        fitness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default() {
        let subject = Video::default();
        assert_eq!(
            subject,
            Video {
                width: None,
                height: None,
                aspect_ratio: None,
                frame_rate: None,
                facing_mode: None,
                resize_mode: None,
            }
        );
    }

    #[test]
    fn builder() {
        let subject = Video::builder()
            .height(Height::exactly(42))
            .frame_rate(FrameRate::at_least(30.0, None))
            .resize_mode(ResizeMode::any_of(
                vec![setting::ResizeMode::CropAndScale],
                None,
            ))
            .build()
            .unwrap();
        assert_eq!(
            subject,
            Video {
                width: None,
                height: Some(Height::exactly(42)),
                aspect_ratio: None,
                frame_rate: Some(FrameRate::at_least(30.0, None)),
                facing_mode: None,
                resize_mode: Some(ResizeMode::any_of(
                    vec![setting::ResizeMode::CropAndScale],
                    None
                )),
            }
        );
    }

    #[test]
    fn fitness_distance() {
        let constraint = Video::builder()
            .height(Height::exactly(42))
            .frame_rate(FrameRate::at_least(30.0, Some(40.0)))
            .resize_mode(ResizeMode::any_of(
                vec![setting::ResizeMode::CropAndScale],
                None,
            ))
            .build()
            .unwrap();

        let setting = setting::Video::builder()
            .height(42)
            .frame_rate(50.0)
            .resize_mode(setting::ResizeMode::CropAndScale)
            .build()
            .unwrap();

        assert_eq!(constraint.fitness_distance(Some(&setting)), 0.2);
    }
}
