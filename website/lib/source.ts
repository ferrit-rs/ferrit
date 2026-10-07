import { articles } from "@/.source/server";
import { loader } from "fumadocs-core/source";

export const source = loader({
  baseUrl: "/articles",
  source: articles.toFumadocsSource(),
});
