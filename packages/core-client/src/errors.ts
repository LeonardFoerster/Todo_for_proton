import type { ErrorDto, ErrorKind } from "./types";

/** Fehler aller `CoreClient`-Methoden; die UI verzweigt nur über `kind`. */
export class CoreClientError extends Error {
  readonly kind: ErrorKind;

  constructor(kind: ErrorKind, message: string) {
    super(message);
    this.name = "CoreClientError";
    this.kind = kind;
  }

  static from(e: unknown): CoreClientError {
    if (e instanceof CoreClientError) return e;
    if (isErrorDto(e)) return new CoreClientError(e.kind, e.message);
    return new CoreClientError("internal", e instanceof Error ? e.message : String(e));
  }
}

function isErrorDto(e: unknown): e is ErrorDto {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}
