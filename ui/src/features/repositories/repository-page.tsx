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

import { getRepository, getRepositoryBranches } from "../../lib/api/client";
import type { BranchSummary, Repository } from "../../lib/api/types";
import { statusColor } from "../../lib/run-status";
import { formatTimestamp, shortCommit } from "../../lib/time";

interface RepositoryPageState {
  repository: Repository | null;
  branches: BranchSummary[];
}

export function RepositoryPage() {
  const { repositoryName } = useParams({
    from: "/repositories/$repositoryName",
  });
  const [state, setState] = useState<RepositoryPageState>({
    repository: null,
    branches: [],
  });
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function load() {
      setIsLoading(true);
      setError(null);

      try {
        const [repository, branches] = await Promise.all([
          getRepository(repositoryName),
          getRepositoryBranches(repositoryName),
        ]);

        if (isMounted) {
          setState({ repository, branches });
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
  }, [repositoryName]);

  if (error) {
    return (
      <Alert
        type="error"
        showIcon
        message="Failed to load repository details"
        description={error}
      />
    );
  }

  if (!isLoading && !state.repository) {
    return (
      <Empty description={`Repository ${repositoryName} was not found.`} />
    );
  }

  return (
    <Space direction="vertical" size="large" className="page-stack">
      <Card loading={isLoading}>
        {state.repository ? (
          <Space direction="vertical" size="middle" className="page-stack">
            <div className="run-title-row">
              <div>
                <Typography.Title level={2}>
                  {state.repository.name}
                </Typography.Title>
                <Typography.Paragraph>
                  {state.repository.path}
                </Typography.Paragraph>
              </div>
              {state.repository.status.last_error ? (
                <Tag color="warning">warning</Tag>
              ) : null}
            </div>

            <Descriptions bordered column={1}>
              <Descriptions.Item label="Configured branches">
                {state.repository.branch_rules.length}
              </Descriptions.Item>
              <Descriptions.Item label="Last seen">
                {formatTimestamp(state.repository.status.last_seen_at_ms)}
              </Descriptions.Item>
              <Descriptions.Item label="Default image">
                <Typography.Text code>
                  {state.repository.pipeline.image}
                </Typography.Text>
              </Descriptions.Item>
            </Descriptions>
          </Space>
        ) : null}
      </Card>

      <Card title="Branches" loading={isLoading}>
        {state.branches.length === 0 && !isLoading ? (
          <Empty description="No branches are configured for this repository." />
        ) : (
          <Table<BranchSummary>
            dataSource={state.branches}
            rowKey={(branch) => branch.branch_name}
            pagination={false}
            columns={[
              {
                title: "Branch",
                key: "branch",
                render: (_, branch) => (
                  <Link
                    to="/repositories/$repositoryName/branches/$branchName"
                    params={{
                      repositoryName,
                      branchName: branch.branch_name,
                    }}
                  >
                    {branch.branch_name}
                  </Link>
                ),
              },
              {
                title: "Status",
                key: "status",
                render: (_, branch) =>
                  branch.latest_run ? (
                    <Tag color={statusColor(branch.latest_run.status)}>
                      {branch.latest_run.status}
                    </Tag>
                  ) : (
                    <Tag>not built</Tag>
                  ),
              },
              {
                title: "Latest commit",
                key: "commit",
                render: (_, branch) =>
                  branch.latest_run
                    ? shortCommit(branch.latest_run.commit_sha)
                    : "Not yet",
              },
              {
                title: "Latest activity",
                key: "activity",
                render: (_, branch) =>
                  branch.latest_run
                    ? formatTimestamp(branch.latest_run.activity_timestamp_ms)
                    : "Not yet",
              },
              {
                title: "Runs",
                dataIndex: "run_count",
                key: "run_count",
              },
            ]}
          />
        )}
      </Card>
    </Space>
  );
}
