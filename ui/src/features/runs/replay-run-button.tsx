import { useState } from "react";
import { Button } from "antd";
import { useNavigate } from "@tanstack/react-router";

import { replayRun } from "../../lib/api/client";

interface ReplayRunButtonProps {
  repositoryId: number | string;
  runId: number | string;
  size?: "small" | "middle" | "large";
  onError?: (message: string) => void;
}

export function ReplayRunButton({
  repositoryId,
  runId,
  size = "middle",
  onError,
}: ReplayRunButtonProps) {
  const navigate = useNavigate();
  const [isReplaying, setIsReplaying] = useState(false);

  async function handleReplay() {
    setIsReplaying(true);

    try {
      const replayedRun = await replayRun(String(runId));
      await navigate({
        to: "/repositories/$repositoryId/runs/$runId",
        params: {
          repositoryId: String(repositoryId),
          runId: String(replayedRun.id),
        },
      });
    } catch (replayError) {
      const message =
        replayError instanceof Error ? replayError.message : "Unknown error";
      onError?.(message);
    } finally {
      setIsReplaying(false);
    }
  }

  return (
    <Button
      size={size}
      loading={isReplaying}
      onClick={() => void handleReplay()}
    >
      Try build again
    </Button>
  );
}
