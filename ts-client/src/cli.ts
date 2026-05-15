import { create } from "@bufbuild/protobuf";
import { Client, Connection } from "@temporalio/client";
import { createRequire } from "node:module";
import { parseArgs } from "node:util";
import { FinishRequestSchema, RunRequestSchema } from "../gen/interop/v1/interop_pb.ts";
import { InteropServiceClient } from "../gen/interop/v1/interop_temporal.ts";

const require = createRequire(import.meta.url);

type RunArgs = {
  targetAddress: string;
  namespace: string;
  caseId: string;
  customerId: string;
  finishReason: string;
};

async function main(): Promise<void> {
  const [command, ...args] = process.argv.slice(2);
  if (command !== "run") {
    throw new Error("usage: npm run cli -- run --target-address HOST:PORT --namespace default --case-id ID --customer-id ID --finish-reason REASON");
  }

  const parsed = parseRunArgs(args);
  const connection = await Connection.connect({ address: parsed.targetAddress });
  try {
    const client = new Client({
      connection,
      namespace: parsed.namespace,
      dataConverter: {
        payloadConverterPath: require.resolve("./data-converter.ts"),
      },
    });

    const interop = new InteropServiceClient(client);
    const input = create(RunRequestSchema, {
      caseId: parsed.caseId,
      customerId: parsed.customerId,
    });
    const workflowId = `interop-${parsed.caseId}`;
    const run = await interop.run(input, { workflowId });
    const status = await waitForStatus(run, parsed.caseId);
    const finish = create(FinishRequestSchema, { reason: parsed.finishReason });
    await run.finish(finish);
    const result = await run.result();

    assertEqual(result.caseId, parsed.caseId, "caseId");
    assertEqual(result.customerId, parsed.customerId, "customerId");
    assertEqual(result.finishReason, parsed.finishReason, "finishReason");
    assertEqual(result.observedStage, "finished", "observedStage");

    process.stdout.write(
      JSON.stringify({
        workflowId: run.workflowId,
        status,
        result,
      }),
    );
    process.stdout.write("\n");
  } finally {
    await connection.close();
  }
}

function parseRunArgs(args: string[]): RunArgs {
  const { values } = parseArgs({
    args,
    options: {
      "target-address": { type: "string", default: "127.0.0.1:7233" },
      namespace: { type: "string", default: "default" },
      "case-id": { type: "string" },
      "customer-id": { type: "string" },
      "finish-reason": { type: "string", default: "ci-finish" },
    },
  });

  return {
    targetAddress: requireValue(values["target-address"], "target-address"),
    namespace: requireValue(values.namespace, "namespace"),
    caseId: requireValue(values["case-id"], "case-id"),
    customerId: requireValue(values["customer-id"], "customer-id"),
    finishReason: requireValue(values["finish-reason"], "finish-reason"),
  };
}

async function waitForStatus(
  run: Awaited<ReturnType<InteropServiceClient["run"]>>,
  caseId: string,
): Promise<{ stage: string; caseId: string }> {
  const deadline = Date.now() + 15_000;
  let lastError: unknown;

  while (Date.now() < deadline) {
    try {
      const status = await run.getStatus();
      if (status.caseId === caseId) {
        return { stage: status.stage, caseId: status.caseId };
      }
      lastError = new Error(`unexpected status caseId ${status.caseId}`);
    } catch (error) {
      lastError = error;
    }
    await delay(250);
  }

  throw new Error(`workflow did not become queryable: ${formatError(lastError)}`);
}

function requireValue(value: string | boolean | undefined, name: string): string {
  if (typeof value === "string" && value.length > 0) {
    return value;
  }
  throw new Error(`missing --${name}`);
}

function assertEqual(actual: string, expected: string, name: string): void {
  if (actual !== expected) {
    throw new Error(`${name}: expected ${expected}, got ${actual}`);
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});

