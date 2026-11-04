use serde::{Deserialize, Serialize};

use crate::error::{DeckError as DriverError, Result};
use crate::model::ActuationSpec;

/// One additive actuation overlay. Layers are applied in priority order, then
/// clamped once to the physical travel range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeadzoneLayer {
    pub id: String,
    pub priority: i32,
    pub delta_mm: f32,
}

/// Effective actuation point after stacking deadzone layers.
///
/// The function is deterministic, commutative for layers with equal priority,
/// and never returns NaN or a value outside the physical range.
pub fn effective_actuation(
    base_mm: f32,
    spec: &ActuationSpec,
    layers: &[DeadzoneLayer],
) -> Result<f32> {
    if !base_mm.is_finite() {
        return Err(DriverError::InvalidArgument {
            field: "base_mm".into(),
            reason: "base actuation must be finite".into(),
        });
    }
    if !spec.min_mm.is_finite()
        || !spec.max_mm.is_finite()
        || !spec.step_mm.is_finite()
        || spec.min_mm <= 0.0
        || spec.max_mm <= spec.min_mm
        || spec.step_mm <= 0.0
    {
        return Err(DriverError::InvalidArgument {
            field: "actuation_spec".into(),
            reason: "actuation range is invalid".into(),
        });
    }

    let mut ordered = layers.to_vec();
    ordered.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));

    let mut value = base_mm;
    for layer in &ordered {
        if !layer.delta_mm.is_finite() {
            return Err(DriverError::InvalidArgument {
                field: "deadzone_layer".into(),
                reason: format!("deadzone layer {} has a non-finite delta", layer.id),
            });
        }
        value += layer.delta_mm;
    }

    if !value.is_finite() {
        return Err(DriverError::InvalidArgument {
            field: "actuation".into(),
            reason: "actuation stacking overflowed".into(),
        });
    }

    let clamped = value.clamp(spec.min_mm, spec.max_mm);
    // Quantise to the device's step size so the UI and device agree.
    let steps = ((clamped - spec.min_mm) / spec.step_mm).round();
    Ok((spec.min_mm + steps * spec.step_mm).clamp(spec.min_mm, spec.max_mm))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ActuationSpec {
        ActuationSpec {
            min_mm: 0.1,
            max_mm: 4.0,
            step_mm: 0.1,
        }
    }

    #[test]
    fn stacks_layers_and_clamps() {
        let layers = vec![
            DeadzoneLayer {
                id: "b".into(),
                priority: 20,
                delta_mm: 0.5,
            },
            DeadzoneLayer {
                id: "a".into(),
                priority: 10,
                delta_mm: -0.1,
            },
        ];
        let value = effective_actuation(1.0, &spec(), &layers).unwrap();
        assert!((value - 1.4).abs() < 0.001);
    }

    #[test]
    fn is_commutative_at_equal_priority() {
        let one = vec![
            DeadzoneLayer {
                id: "z".into(),
                priority: 1,
                delta_mm: 0.2,
            },
            DeadzoneLayer {
                id: "a".into(),
                priority: 1,
                delta_mm: -0.1,
            },
        ];
        let mut two = one.clone();
        two.reverse();
        assert_eq!(
            effective_actuation(2.0, &spec(), &one).unwrap(),
            effective_actuation(2.0, &spec(), &two).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(effective_actuation(f32::NAN, &spec(), &[]).is_err());
        assert!(effective_actuation(
            1.0,
            &spec(),
            &[DeadzoneLayer {
                id: "bad".into(),
                priority: 0,
                delta_mm: f32::INFINITY,
            }]
        )
        .is_err());
    }

    #[test]
    fn clamps_below_minimum() {
        let layers = vec![DeadzoneLayer {
            id: "negative".into(),
            priority: 0,
            delta_mm: -10.0,
        }];
        assert!((effective_actuation(1.0, &spec(), &layers).unwrap() - 0.1).abs() < 0.001);
    }
}
