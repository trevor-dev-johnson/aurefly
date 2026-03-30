import { NextResponse } from "next/server";

import { BackendApiError, backendFetch, requireSessionAccessToken } from "@/lib/backend";
import type { CreateInvoicePayload, MerchantInvoice } from "@/lib/aurefly-api";

export async function GET() {
  try {
    const accessToken = await requireSessionAccessToken();
    const invoices = await backendFetch<MerchantInvoice[]>("/me/invoices", {
      accessToken,
    });

    return NextResponse.json(invoices);
  } catch (error) {
    return toErrorResponse(error);
  }
}

export async function POST(request: Request) {
  try {
    const accessToken = await requireSessionAccessToken();
    const payload = (await request.json()) as CreateInvoicePayload;
    const invoice = await backendFetch<MerchantInvoice>("/me/invoices", {
      method: "POST",
      body: payload,
      accessToken,
    });

    return NextResponse.json(invoice, { status: 201 });
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

