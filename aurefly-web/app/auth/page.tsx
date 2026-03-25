import { AuthClient } from "./auth-client";

export const metadata = {
  title: "Aurefly Auth",
};

type AuthPageProps = {
  searchParams: Promise<{
    mode?: string;
  }>;
};

export default async function AuthPage({ searchParams }: AuthPageProps) {
  const params = await searchParams;

  return <AuthClient initialMode={params.mode} />;
}
