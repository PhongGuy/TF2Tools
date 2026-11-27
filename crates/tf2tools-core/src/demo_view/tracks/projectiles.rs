//! Every projectile's path over a whole demo.

/// What a projectile is, as the viewer draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProjectileKind {
    /// A rocket, from a launcher or a sentry.
    Rocket,
    /// A grenade launcher's pipe.
    Pipe,
    /// A stickybomb.
    Sticky,
    /// A Huntsman arrow or a Crusader's Crossbow bolt.
    Arrow,
    /// A flare.
    Flare,
    /// A Cow Mangler shot.
    EnergyBall,
    /// A Righteous Bison or Pomson shot.
    EnergyRing,
    /// A jar, cleaver or baseball.
    Thrown,
    /// Anything else that flies.
    Other,
}
