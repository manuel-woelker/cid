import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  Card,
  Col,
  Empty,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
} from "antd";
import { Link } from "@tanstack/react-router";

import { getRepositories, getRuns, getSummary } from "../../lib/api/client";
import type { Repository, Run, Summary } from "../../lib/api/types";
import { statusColor } from "../../lib/run-status";
import { formatTimestamp, shortCommit } from "../../lib/time";

interface DashboardState {
  repositories: Repository[];
  runs: Run[];
  summary: Summary | null;
}

export function DashboardPage() {
  const [state, setState] = useState<DashboardState>({
    repositories: [],
    runs: [],
    summary: null,
  });
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function load() {
      setIsLoading(true);
      setError(null);

      try {
        const [repositories, runs, summary] = await Promise.all([
          getRepositories(),
          getRuns(),
          getSummary(),
        ]);

        if (!isMounted) {
          return;
        }

        setState({ repositories, runs, summary });
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
  }, []);

  const recentRuns = useMemo(
    () =>
      [...state.runs].sort((left, right) => right.id - left.id).slice(0, 10),
    [state.runs],
  );

  return (
    <Space direction="vertical" size="large" className="page-stack">
      <section className="page-hero">
        <Typography.Title>
          Local run visibility without the hosted-CI tax
        </Typography.Title>
        <Typography.Paragraph>
          Track watched repositories, see what is running, and jump straight to
          the run that broke your branch.
        </Typography.Paragraph>
      </section>

      {error ? (
        <Alert
          type="error"
          showIcon
          message="Failed to load dashboard data"
          description={error}
        />
      ) : null}

      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={8}>
          <Card className="stat-card" loading={isLoading}>
            <Statistic
              title="Tracked repositories"
              value={state.repositories.length}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={8}>
          <Card className="stat-card" loading={isLoading}>
            <Statistic
              title="Recent runs"
              value={state.summary?.total_runs ?? 0}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={8}>
          <Card className="stat-card" loading={isLoading}>
            <Statistic
              title="Passing runs"
              value={state.summary?.passed_runs ?? 0}
              suffix={`/ ${state.summary?.total_runs ?? 0}`}
            />
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]}>
        <Col xs={24} xl={10}>
          <Card
            title="Repositories"
            extra={
              state.summary ? (
                <Space size="small">
                  <Tag color="processing">
                    running {state.summary.running_runs}
                  </Tag>
                  <Tag color="warning">queued {state.summary.queued_runs}</Tag>
                </Space>
              ) : null
            }
          >
            {state.repositories.length === 0 && !isLoading ? (
              <Empty description="No repositories are registered yet." />
            ) : (
              <Space
                direction="vertical"
                size="middle"
                className="repository-list"
              >
                {state.repositories.map((repository) => (
                  <Card
                    key={repository.id}
                    size="small"
                    className="repository-card"
                    title={
                      <Link
                        to="/repositories/$repositoryId"
                        params={{ repositoryId: String(repository.id) }}
                      >
                        {repository.name}
                      </Link>
                    }
                  >
                    <Typography.Paragraph className="muted-text">
                      {repository.path}
                    </Typography.Paragraph>
                    <Space wrap>
                      {repository.branch_rules.map((rule) => (
                        <Link
                          key={rule.branch}
                          to="/repositories/$repositoryId/branches/$branchName"
                          params={{
                            repositoryId: String(repository.id),
                            branchName: rule.branch,
                          }}
                        >
                          <Tag>{rule.branch}</Tag>
                        </Link>
                      ))}
                    </Space>
                    <Typography.Paragraph className="muted-text compact-paragraph">
                      Last seen:{" "}
                      {formatTimestamp(repository.status.last_seen_at_ms)}
                    </Typography.Paragraph>
                    {repository.status.last_error ? (
                      <Alert
                        className="inline-alert"
                        type="warning"
                        showIcon
                        message={repository.status.last_error}
                      />
                    ) : null}
                  </Card>
                ))}
              </Space>
            )}
          </Card>
        </Col>
        <Col xs={24} xl={14}>
          <Card title="Recent runs">
            <Table<Run>
              loading={isLoading}
              dataSource={recentRuns}
              rowKey={(run) => run.id}
              pagination={false}
              locale={{
                emptyText: <Empty description="No runs recorded yet." />,
              }}
              columns={[
                {
                  title: "Run",
                  key: "run",
                  render: (_, run) => (
                    <Space direction="vertical" size={0}>
                      <Link
                        to="/runs/$runId"
                        params={{ runId: String(run.id) }}
                      >
                        #{run.id} {run.repository_name}
                      </Link>
                      <Typography.Text type="secondary">
                        {run.branch} · {shortCommit(run.commit_sha)}
                      </Typography.Text>
                    </Space>
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
                  title: "Queued",
                  dataIndex: "queued_at_ms",
                  key: "queued_at_ms",
                  render: (value: number) => formatTimestamp(value),
                },
                {
                  title: "Steps",
                  key: "steps",
                  render: (_, run) =>
                    `${run.steps.length} step${run.steps.length === 1 ? "" : "s"}`,
                },
              ]}
            />
          </Card>
        </Col>
      </Row>
    </Space>
  );
}
