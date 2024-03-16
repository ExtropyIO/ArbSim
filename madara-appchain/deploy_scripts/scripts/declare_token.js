const starknet = require("starknet");

const provider = new starknet.RpcProvider({
  nodeUrl: "http://localhost:9944",
});

const account = new starknet.Account(
  provider,
  "0x4",
  "0x00c1cf1490de1352865301bb8705143f3ef938f97fdf892f1090dcb5ac7bcd1d",
  "1",
);

async function main(sierraPath) {
  const currentDir = process.cwd();
  const sierra = require(`${currentDir}/${sierraPath}`);

  try {
    const declareResult = await account.declare({
      contract: sierra,
    });
    console.log("This is the declare result - ", declareResult);
  } catch (err) {
    console.log("Error during declaration: ", err.message);
  }
}

main(process.argv[2]);
