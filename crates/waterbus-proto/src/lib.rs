pub mod sfu {
    tonic::include_proto!("sfu");
}

pub mod dispatcher {
    tonic::include_proto!("dispatcher");
}

pub use sfu::*;
pub use dispatcher::*;