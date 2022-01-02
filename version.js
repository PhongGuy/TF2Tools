const { error, log } = require("console");
const fs = require("fs");

const p1Path = "./package.json";
const p2Path = "./app/package.json";
const envPath = "./src/environments";

const newVersion = process.argv[2];

if (newVersion === undefined) {
  error("new version has to be specified");
} else {
  // update package.json
  const p1 = fs.readFileSync(p1Path, { encoding: "utf8", flag: "r" });
  const p2 = fs.readFileSync(p2Path, { encoding: "utf8", flag: "r" });

  const v1 = p1.split("\n").filter((a) => a.includes('"version":'))[0];
  const v2 = p2.split("\n").filter((a) => a.includes('"version":'))[0];

  const v = v1
    .replace("version", "")
    .replace(/"/g, "")
    .replace(/:/g, "")
    .replace(/,/g, "")
    .replace(/ /g, "");

  const newV1 = v1.replace(v, newVersion);
  const newV2 = v2.replace(v, newVersion);

  fs.writeFileSync(p1Path, p1.replace(v1, newV1));
  fs.writeFileSync(p2Path, p2.replace(v2, newV2));

  // update APP_CONFIG
  const envs = fs.readdirSync(envPath);

  envs.forEach((env) => {
    const p = `${envPath}/${env}`;
    const e = fs.readFileSync(p, { encoding: "utf8", flag: "r" });
    fs.writeFileSync(p, e.replace(v, newVersion));
    log(`Updated ${env}`);
  });
}
