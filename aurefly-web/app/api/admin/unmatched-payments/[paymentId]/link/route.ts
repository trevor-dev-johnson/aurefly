import { NextResponse } from "next/server";

import { BackendApiError, backendFetch, requireSessionAccessToken } from "@/lib/backend";
import type { UnmatchedPaymentDetail } from "@/lib/aurefly-api";

type RouteContext = {
  params: Promise<{
    paymentId: string;
  }>;
};

export async function POST(request: Request, context: RouteContext) {
  try {
    const accessToken = await requireSessionAccessToken();
    const { paymentId } = await context.params;
    const payload = await request.json().catch(() => ({}));
    const detail = await backendFetch<UnmatchedPaymentDetail>(
      `/admin/unmatched-payments/${paymentId}/link`,
      {
        method: "POST",
        body: payload,
        accessToken,
      },
    );

    return NextResponse.json(detail);
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
