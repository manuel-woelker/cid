import { useEffect, useState } from "react";
import {
  Alert,
  Card,
  Descriptions,
  Empty,
  Space,
  Table,
  Tag,
  Typography,
} from "antd";
import { Link, useParams } from "@tanstack/react-router";

import { getRepositoryBranch } from "../../lib/api/client";
import type { BranchDetail, Run } from "../../lib/api/types";
import { statusColor } from "../../lib/run-status";
import { formatTimestamp, shortCommit } from "../../lib/time";

export function BranchPage() {
  const { repositoryId, branchName } = useParams({
    from: "/repositories/$repositoryId/branches/$branchName",
  });
  const [detail, setDetail] = useState<BranchDetail | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function load() {
      setIsLoading(true);
      setError(null);

      try {
        const nextDetail = await getRepositoryBranch(repositoryId, branchName);
        if (isMounted) {
          setDetail(nextDetail);
        }
      } catch (loadError) {
        if (!isMounted) {
          return;
        }

        setError(
          loadError instanceof Error ? loadError.message : "Unknown error",
        );
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
  }, [repositoryId, branchName]);

  if (error) {
    return (
      <Alert
        type="error"
        showIcon
        message="Failed to load branch details"
        description={error}
      />
    );
  }

  if (!isLoading && !detail) {
    return <Empty description={`Branch ${branchName} was not found.`} />;
  }

  return (
    <Space direction="vertical" size="large" className="page-stack">
      <Card loading={isLoading}>
        {detail ? (
          <Space direction="vertical" size="middle" className="page-stack">
            <Space direction="vertical" size={0}>
              <Typography.Text type="secondary">
                <Link
                  to="/repositories/$repositoryId"
                  params={{ repositoryId: String(detail.repository.id) }}
                >
                  {detail.repository.name}
                </Link>
              </Typography.Text>
              <Typography.Title level={2}>
                {detail.branch.branch_name}
              </Typography.Title>
            </Space>

            <Descriptions bordered column={1}>
              <Descriptions.Item label="Latest status">
                {detail.branch.latest_run ? (
                  <Tag color={statusColor(detail.branch.latest_run.status)}>
                    {detail.branch.latest_run.status}
                  </Tag>
                ) : (
                  <Tag>not built</Tag>
                )}
              </Descriptions.Item>
              <Descriptions.Item label="Latest activity">
                {detail.branch.latest_run
                  ? formatTimestamp(
                      detail.branch.latest_run.activity_timestamp_ms,
                    )
                  : "Not yet"}
              </Descriptions.Item>
              <Descriptions.Item label="Runs">
                {detail.branch.run_count}
              </Descriptions.Item>
            </Descriptions>
          </Space>
        ) : null}
      </Card>

      <Card title="Branch runs" loading={isLoading}>
        {detail && detail.runs.length > 0 ? (
          <Table<Run>
            dataSource={detail.runs}
            rowKey={(run) => run.id}
            pagination={false}
            columns={[
              {
                title: "Run",
                key: "run",
                render: (_, run) => (
                  <Link
                    to="/repositories/$repositoryId/runs/$runId"
                    params={{
                      repositoryId: String(run.repository_id),
                      runId: String(run.id),
                    }}
                  >
                    #{run.id}
                  </Link>
                ),
              },
              {
                title: "Status",
                dataIndex: "status",
                key: "status",
                render: (status: Run["status"]) => (
                  <Tag color={statusColor(status)}>{status}</Tag>
                ),
              },
              {
                title: "Commit",
                key: "commit",
                render: (_, run) => shortCommit(run.commit_sha),
              },
              {
                title: "Queued",
                dataIndex: "queued_at_ms",
                key: "queued_at_ms",
                render: (value: number) => formatTimestamp(value),
              },
              {
                title: "Finished",
                dataIndex: "finished_at_ms",
                key: "finished_at_ms",
                render: (value: number | null) => formatTimestamp(value),
              },
            ]}
          />
        ) : (
          <Empty description="No runs recorded for this branch yet." />
        )}
      </Card>
    </Space>
  );
}
