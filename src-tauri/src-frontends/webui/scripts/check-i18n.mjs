import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const root = path.resolve("src");
const localeRoot = path.join(root, "i18n", "locales");

function readLocale(name) {
  const entries = new Map();
  const file = path.join(localeRoot, `${name}.ts`);
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const match = line.match(/^\s+"([^"]+)": ("(?:[^"\\]|\\.)*"),$/);
    if (match) entries.set(match[1], JSON.parse(match[2]));
  }
  return entries;
}

function placeholders(value) {
  return [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort().join(",");
}

const zh = readLocale("zh-CN");
const en = readLocale("en-US");
const errors = [];

for (const [key, value] of zh) {
  if (!en.has(key)) errors.push(`en-US is missing ${key}`);
  else if (placeholders(value) !== placeholders(en.get(key))) errors.push(`placeholder mismatch for ${key}`);
}
for (const key of en.keys()) if (!zh.has(key)) errors.push(`zh-CN is missing ${key}`);
for (const [key, value] of en) if (/\p{Script=Han}/u.test(value)) errors.push(`en-US contains Chinese text at ${key}`);

function walk(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) return entry.name === "i18n" ? [] : walk(target);
    if (!/\.(ts|tsx)$/.test(entry.name) || /\.(test|spec)\.(ts|tsx)$/.test(entry.name)) return [];
    return [target];
  });
}

for (const file of walk(root)) {
  const sourceText = fs.readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true, file.endsWith("x") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  function visit(node) {
    const visibleLiteral = ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isJsxText(node);
    if (visibleLiteral && /\p{Script=Han}/u.test(node.text ?? "")) {
      errors.push(`${path.relative(root, file)}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1} contains hardcoded Chinese UI text`);
    }
    if (ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) {
      if (/\p{Script=Han}/u.test(node.text)) errors.push(`${path.relative(root, file)} contains hardcoded Chinese template text`);
    }
    if (
      ts.isCallExpression(node)
      && ts.isIdentifier(node.expression)
      && node.expression.text === "t"
      && node.arguments.length > 0
      && ts.isStringLiteral(node.arguments[0])
      && !zh.has(node.arguments[0].text)
    ) {
      errors.push(`${path.relative(root, file)} references missing key ${node.arguments[0].text}`);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`i18n check passed: ${zh.size} keys across 2 language files`);
