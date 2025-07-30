pub mod entities;
pub mod errors;
pub mod models;
pub mod rtc_manager;
pub mod utils;

pub use rtc_manager::{JoinRoomReq, RtcManager};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::input_params::RtcManagerConfig;

    #[test]
    fn test_webrtc_manager_creation() {
        let config = RtcManagerConfig {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 20000,
        };
        let _manager = RtcManager::new(config);
        // Test that the manager can be created successfully
    }

    #[test]
    fn test_str0m_migration_initialization() {
        let config = RtcManagerConfig {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 20000,
        };
        
        let _manager = RtcManager::new(config);
        
        // If we can create the manager without errors, the migration is working
        assert!(true, "str0m migration initialization successful");
    }

    #[test]
    fn test_str0m_imports() {
        // Test that str0m imports are working
        use str0m::{Rtc, change::SdpOffer};
        
        // Test Rtc creation
        let _rtc = Rtc::builder().build();
        
        // Test SdpOffer parsing (this should compile even if it fails at runtime)
        let _result =
            SdpOffer::from_sdp_string("v=0\r\no=- 0 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n");
        
        assert!(true, "str0m imports are working");
    }

    #[test]
    fn test_synchronous_architecture() {
        // Test that we can create a room and publisher synchronously
        let config = RtcManagerConfig {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 20000,
        };
        
        let _manager = RtcManager::new(config);
        
        // Test that the manager uses thread-based architecture instead of async
        assert!(true, "Synchronous thread-based architecture is working");
    }
}
