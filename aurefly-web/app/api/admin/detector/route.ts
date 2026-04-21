import { NextResponse } from "next/server";

import { BackendApiError, backendFetch, requireSessionAccessToken } from "@/lib/backend";
import type { DetectorStatus } from "@/lib/aurefly-api";

export async function GET() {
  try {
    const accessToken = await requireSessionAccessToken();
    const status = await backendFetch<DetectorStatus>("/admin/detector", {
      accessToken,
    });

    return NextResponse.json(status);
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
