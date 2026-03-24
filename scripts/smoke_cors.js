const base = process.argv[2] || "http://localhost:8080/api/v1/health";
const origin = process.argv[3] || "https://aurefly.com";

async function main() {
  const response = await fetch(base, {
    headers: {
      Origin: origin,
    },
  });

  console.log(
    JSON.stringify(
      {
        origin,
        status: response.status,
        allow_origin: response.headers.get("access-control-allow-origin"),
        vary: response.headers.get("vary"),
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
