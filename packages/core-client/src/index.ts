export type {
  AuthClient,
  BackgroundClient,
  CoreClient,
  DataClient,
  NotesClient,
  ShellAction,
  SyncClient,
} from "./core-client";
export { CoreClientError } from "./errors";
export { FakeCoreClient, type FakeOptions } from "./fake-core-client";
export type * from "./types";
