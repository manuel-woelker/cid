import { useEffect, useState } from "react";
import {
  Alert,
  Card,
  Descriptions,
  Empty,
  Space,
  Steps,
  Tag,
  Timeline,
  Typography,
} from "antd";
import { useParams } from "@tanstack/react-router";

import { getRun } from "../../lib/api/client";
import type { Run } from "../../lib/api/types";
import { formatDuration, formatTimestamp, shortCommit } from "../../lib/time";

function statusColor(status: Run["status"]): string {
  switch (status) {
    case "passed":
      return "success";
    case "failed":
      return "error";
    case "running":
      return "processing";
    case "queued":
      return "warning";
    case "canceled":
      return "default";
  }
}

export function RunDetailPage() {
  const { runId } = useParams({ from: "/runs/$runId" });
  const [run, setRun] = useState<Run | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function load() {
      setIsLoading(true);
      setError(null);

      try {
        const nextRun = await getRun(runId);
        if (isMounted) {
          setRun(nextRun);
        }
      } catch (loadError) {
        if (!isMounted) {
          return;
        }

        const message =
          loadError instanceof Error ? loadError.message : "Unknown error";
        setError(message);
      } finally {
        if (isMounted) {
          setIsLoading(false);
        }
      }
    }

    void load();

    return () => {
      isMounted = false;
    };
  }, [runId]);

  if (error) {
    return (
      <Alert
        type="error"
        showIcon
        message="Failed to load run details"
        description={error}
      />
    );
  }

  if (!isLoading && !run) {
    return <Empty description={`Run ${runId} was not found.`} />;
  }

  return (
    <Space direction="vertical" size="large" className="page-stack">
      <Card loading={isLoading}>
        {run ? (
          <Space direction="vertical" size="middle" className="page-stack">
            <div className="run-title-row">
              <div>
                <Typography.Title level={2}>
                  Run #{run.id} · {run.repository_name}
                </Typography.Title>
                <Typography.Paragraph>
                  {run.branch} · {shortCommit(run.commit_sha)}
                </Typography.Paragraph>
              </div>
              <Tag color={statusColor(run.status)}>{run.status}</Tag>
            </div>

            <Descriptions bordered column={1}>
              <Descriptions.Item label="Queued at">
                {formatTimestamp(run.queued_at_ms)}
              </Descriptions.Item>
              <Descriptions.Item label="Started at">
                {formatTimestamp(run.started_at_ms)}
              </Descriptions.Item>
              <Descriptions.Item label="Finished at">
                {formatTimestamp(run.finished_at_ms)}
              </Descriptions.Item>
              <Descriptions.Item label="Steps">
                {run.steps.length}
              </Descriptions.Item>
            </Descriptions>
          </Space>
        ) : null}
      </Card>

      <Card title="Step progress" loading={isLoading}>
        {run ? (
          <Steps
            direction="vertical"
            current={run.steps.findIndex((step) => step.status !== "passed")}
            items={run.steps.map((step) => ({
              title: step.name,
              status:
                step.status === "failed"
                  ? "error"
                  : step.status === "passed"
                    ? "finish"
                    : step.status === "running"
                      ? "process"
                      : "wait",
              description: `${step.command} · ${formatDuration(step.duration_ms)}`,
            }))}
          />
        ) : (
          <Empty description="No step data available." />
        )}
      </Card>

      <Card title="Step details" loading={isLoading}>
        {run ? (
          <Space direction="vertical" size="middle" className="page-stack">
            {run.steps.map((step) => (
              <Card key={step.name} size="small">
                <Descriptions bordered column={1} size="small">
                  <Descriptions.Item label="Status">
                    <Tag color={statusColor(step.status)}>{step.status}</Tag>
                  </Descriptions.Item>
                  <Descriptions.Item label="Command">
                    <Typography.Text code>{step.command}</Typography.Text>
                  </Descriptions.Item>
                  <Descriptions.Item label="Image">
                    <Typography.Text code>{step.image}</Typography.Text>
                  </Descriptions.Item>
                  <Descriptions.Item label="Exit code">
                    {step.exit_code ?? "Not finished"}
                  </Descriptions.Item>
                  <Descriptions.Item label="Duration">
                    {formatDuration(step.duration_ms)}
                  </Descriptions.Item>
                  <Descriptions.Item label="Log path">
                    {step.log_path ? (
                      <Typography.Text code>{step.log_path}</Typography.Text>
                    ) : (
                      "Not written"
                    )}
                  </Descriptions.Item>
                </Descriptions>
              </Card>
            ))}
          </Space>
        ) : (
          <Empty description="No step details available." />
        )}
      </Card>

      <Card title="Run events" loading={isLoading}>
        {run && run.events.length > 0 ? (
          <Timeline
            items={run.events.map((event) => ({
              children: `${formatTimestamp(event.timestamp_ms)} · ${event.message}`,
            }))}
          />
        ) : (
          <Empty description="No event history is available." />
        )}
      </Card>
    </Space>
  );
}
