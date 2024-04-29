import {
  Notebook,
  TextCell,
  CodeCell,
  PrometheusCell,
} from "https://raw.githubusercontent.com/keturiosakys/template-api/main/mod.ts";

export default function Template() {
	console.log("Template function called");
  return Notebook({
    title: "A TYPESCRIPT TEMPLATE WOOO",
    frontMatter: {},
    cells: [
      TextCell("Hello, world!"),
			TextCell("Another text cell"),
      CodeCell("js", "print('Hello, world!')"),
      PrometheusCell("up"),
    ],
  });
}
 
// Why does this make sense?
//
// - familiarity, ppl know typescript more than jsonnet
//
// - types! and language servers 
// 				reusable from our internal models, give immediate feedback in the editor
//
// - actual programming language


