//! Analysis tracker implementations.
//!
//! Each tracker implements the `AnalysisTracker` trait and processes
//! MBO events to produce a specific statistical profile.

pub mod cross_scale_ofi;
pub mod depth;
pub mod jumps;
pub mod lifecycle;
pub mod liquidity;
pub mod noise;
pub mod ofi;
pub mod quality;
pub mod returns;
pub mod spread;
pub mod trades;
pub mod volatility;
pub mod vpin;

pub use cross_scale_ofi::CrossScaleOfiTracker;
pub use depth::DepthTracker;
pub use jumps::JumpTracker;
pub use lifecycle::LifecycleTracker;
pub use liquidity::LiquidityTracker;
pub use noise::NoiseTracker;
pub use ofi::OfiTracker;
pub use quality::QualityTracker;
pub use returns::ReturnTracker;
pub use spread::SpreadTracker;
pub use trades::TradeTracker;
pub use volatility::VolatilityTracker;
pub use vpin::VpinTracker;
