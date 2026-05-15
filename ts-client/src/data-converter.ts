import { createProtobufEsPayloadConverter } from "@nu-sync/temporal-protobuf-es";
import { schemas as interopSchemas } from "../gen/interop/v1/interop_pb_register.ts";

export const payloadConverter = createProtobufEsPayloadConverter({
  schemas: interopSchemas,
  encoding: "binary",
});

