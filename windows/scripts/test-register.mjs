// Use the project's existing TypeScript compiler for Node's test runner.
// No test framework or production dependency is needed for the pure session core.
import { register } from "node:module";
register("./test-loader.mjs", import.meta.url);
