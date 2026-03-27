pub mod client;
pub mod codec;

// Generated protobuf types from official Volcengine proto definitions
#[allow(dead_code)]
pub mod proto {
    pub mod event {
        include!(concat!(env!("OUT_DIR"), "/data.speech.event.rs"));
    }
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/data.speech.common.rs"));
    }
    #[allow(dead_code)]
    pub mod understanding {
        include!(concat!(env!("OUT_DIR"), "/data.speech.understanding.rs"));
    }
    pub mod ast {
        include!(concat!(env!("OUT_DIR"), "/data.speech.ast.rs"));
    }
}
