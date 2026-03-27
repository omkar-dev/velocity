pub mod device;
pub mod inspect;
pub mod lint;
pub mod mcp;
pub mod migrate;
pub mod run;
pub mod validate;

use velocity_common::{Platform, PlatformDriver};

pub fn create_driver(platform: Platform) -> Box<dyn PlatformDriver> {
    match platform {
        Platform::Ios => Box::new(velocity_ios::IosDriver::new()),
        Platform::Android => Box::new(velocity_android::AndroidDriver::new()),
    }
}
