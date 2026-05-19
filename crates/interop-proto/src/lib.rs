#![allow(clippy::all)]

pub use temporal_proto_runtime::{ProtoEmpty, TemporalProtoMessage, TypedProtoMessage};

pub mod interop {
    pub mod v1 {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/gen/interop/v1/interop.v1.rs"
        ));
    }
}

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/gen/interop/v1/interop_temporal.rs"
));
