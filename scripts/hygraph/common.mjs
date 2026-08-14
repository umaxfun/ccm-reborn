export function requireEnvironment(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required. Add it to .env or the CI environment.`);
  return value;
}

export function hasFlag(flag) {
  return process.argv.slice(2).includes(flag);
}

export function flagValues(flag) {
  const values = [];
  for (let index = 2; index < process.argv.length; index += 1) {
    if (process.argv[index] === flag) {
      const value = process.argv[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${flag} requires a value.`);
      values.push(value);
      index += 1;
    }
  }
  return values;
}

export async function graphQlRequest({ endpoint, token, query, variables = {} }) {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ query, variables }),
    });
    const result = await response.json().catch(() => null);
    const detail = result?.errors?.map((error) => error.message).join("; ") ?? `HTTP ${response.status}`;
    const rateLimited = response.status === 429 || /too many requests|rate limit/i.test(detail);
    if (rateLimited && attempt < 5) {
      const retryAfterSeconds = Number(response.headers.get("retry-after"));
      const retryDelay = Number.isFinite(retryAfterSeconds) && retryAfterSeconds > 0
        ? retryAfterSeconds * 1000
        : 1_000 * (attempt + 1);
      await new Promise((resolvePromise) => setTimeout(resolvePromise, retryDelay));
      continue;
    }
    if (!response.ok || result?.errors?.length) {
      throw new Error(`Hygraph GraphQL request failed: ${detail}`);
    }
    if (!result?.data) throw new Error("Hygraph GraphQL request returned no data.");
    return result.data;
  }
  throw new Error("Hygraph GraphQL request exhausted retries.");
}
