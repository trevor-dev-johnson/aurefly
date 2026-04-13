import { NextResponse } from "next/server";

import { BackendApiError, backendFetch, requireSessionAccessToken } from "@/lib/backend";
import type { UnmatchedPaymentSummary } from "@/lib/aurefly-api";

export async function GET(request: Request) {
  try {
    const accessToken = await requireSessionAccessToken();
    const url = new URL(request.url);
    const query = url.searchParams.toString();
    const path = query
      ? `/admin/unmatched-payments?${query}`
      : "/admin/unmatched-payments";
    const payments = await backendFetch<UnmatchedPaymentSummary[]>(path, {
      accessToken,
    });

    return NextResponse.json(payments);
  } catch (error) {
    return toErrorResponse(error);
  }
}

function toErrorResponse(error: unknown) {
  if (error instanceof BackendApiError) {
    return NextResponse.json({ error: error.message }, { status: error.status });
  }

  return NextResponse.json({ error: "Internal server error." }, { status: 500 });
}
