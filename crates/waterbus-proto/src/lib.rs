pub mod common {
    tonic::include_proto!("common");
}

pub mod sfu {
    tonic::include_proto!("sfu");
}

pub mod dispatcher {
    tonic::include_proto!("dispatcher");
}

pub use common::*;
pub use sfu::*;
pub use dispatcher::*;