// Worker entry: only the handler may be exported here (the logic and its tests: feed.js).
import { handle } from "./feed.js";

export default {
  fetch(req, env) {
    return handle(req, env);
  },
};
