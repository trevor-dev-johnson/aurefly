import { NextResponse } from "next/server";

import { BackendApiError, backendFetch, requireSessionAccessToken } from "@/lib/backend";
import type { MerchantInvoice } from "@/lib/aurefly-api";

type RouteContext = {
  params: Promise<{
    invoiceId: string;
  }>;
};

export async function POST(_: Request, context: RouteContext) {
  try {
    const { invoiceId } = await context.params;
    const accessToken = await requireSessionAccessToken();
    const invoice = await backendFetch<MerchantInvoice>(`/me/invoices/${invoiceId}/cancel`, {
      method: "POST",
      accessToken,
    });

    return NextResponse.json(invoice);
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
