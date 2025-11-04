import { useState, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { 
  Activity, 
  Database, 
  Zap, 
  Clock, 
  TrendingUp, 
  Server,
  HardDrive,
  Cpu,
  BarChart3
} from 'lucide-react';

interface MetricData {
  storageReads: number;
  storageWrites: number;
  storageDeletes: number;
  cacheHits: number;
  cacheMisses: number;
  cacheHotHits: number;
  cacheWarmHits: number;
  cacheSizeHot: number;
  cacheSizeWarm: number;
  avgReadLatency: number;
  avgWriteLatency: number;
  avgDeleteLatency: number;
  graphqlRequests: number;
  graphqlErrors: number;
}

interface HistoricalData {
  timestamp: number;
  reads: number;
  writes: number;
  latency: number;
  cacheHitRate: number;
}

export default function Metrics() {
  const [history, setHistory] = useState<HistoricalData[]>([]);
  const [autoRefresh, setAutoRefresh] = useState(true);

  // Fetch real metrics from GraphQL endpoint
  const { data: metrics } = useQuery<MetricData>({
    queryKey: ['metrics'],
    queryFn: async () => {
      const response = await fetch('http://localhost:8080/graphql', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          query: `
            query {
              getMetrics {
                storageReads
                storageWrites
                storageDeletes
                cacheHits
                cacheMisses
                cacheHotHits
                cacheWarmHits
                cacheSizeHot
                cacheSizeWarm
                readLatencyAvg
                writeLatencyAvg
                deleteLatencyAvg
                graphqlRequests
                graphqlErrors
              }
            }
          `,
        }),
      });

      if (!response.ok) {
        throw new Error('Failed to fetch metrics');
      }

      const result = await response.json();
      console.log('Metrics response:', result);
      
      if (result.errors) {
        console.error('GraphQL errors:', result.errors);
        throw new Error(result.errors[0].message);
      }
      
      const data = result.data.getMetrics;

      return {
        storageReads: data.storageReads,
        storageWrites: data.storageWrites,
        storageDeletes: data.storageDeletes,
        cacheHits: data.cacheHits,
        cacheMisses: data.cacheMisses,
        cacheHotHits: data.cacheHotHits,
        cacheWarmHits: data.cacheWarmHits,
        cacheSizeHot: data.cacheSizeHot,
        cacheSizeWarm: data.cacheSizeWarm,
        avgReadLatency: data.readLatencyAvg,
        avgWriteLatency: data.writeLatencyAvg,
        avgDeleteLatency: data.deleteLatencyAvg,
        graphqlRequests: data.graphqlRequests,
        graphqlErrors: data.graphqlErrors,
      };
    },
    refetchInterval: autoRefresh ? 2000 : false,
  });

  // Update history for charts
  useEffect(() => {
    if (metrics) {
      const totalCache = metrics.cacheHits + metrics.cacheMisses;
      const cacheHitRate = totalCache > 0 ? (metrics.cacheHits / totalCache) * 100 : 0;
      
      setHistory(prev => {
        const newPoint: HistoricalData = {
          timestamp: Date.now(),
          reads: metrics.storageReads,
          writes: metrics.storageWrites,
          latency: metrics.avgReadLatency,
          cacheHitRate: cacheHitRate,
        };
        
        // Keep last 50 points
        const updated = [...prev, newPoint].slice(-50);
        return updated;
      });
    }
  }, [metrics]);

  const cacheHitRate = metrics 
    ? (() => {
        const total = metrics.cacheHits + metrics.cacheMisses;
        return total > 0 ? ((metrics.cacheHits / total) * 100).toFixed(2) : '0';
      })()
    : '0';

  const errorRate = metrics
    ? (() => {
        return metrics.graphqlRequests > 0 
          ? ((metrics.graphqlErrors / metrics.graphqlRequests) * 100).toFixed(2) 
          : '0';
      })()
    : '0';

  // Show loading or error states
  if (!metrics) {
    return (
      <div className="p-6">
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-8 text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600 dark:text-gray-400">Loading metrics...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between pr-48">
        <div>
          <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Performance Metrics</h1>
          <p className="text-gray-600 dark:text-gray-400 mt-1">Real-time system performance monitoring</p>
        </div>
        <div className="flex items-center gap-4">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded border-gray-300 dark:border-gray-600"
            />
            <span className="text-sm text-gray-700 dark:text-gray-300">Auto-refresh</span>
          </label>
          <div className="flex items-center gap-2 px-3 py-1 bg-green-100 dark:bg-green-900/20 rounded-full">
            <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
            <span className="text-sm font-medium text-green-700 dark:text-green-400">Live</span>
          </div>
        </div>
      </div>

      {/* Quick Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <MetricCard
          icon={<Database className="w-6 h-6 text-blue-600" />}
          title="Total Operations"
          value={metrics ? (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes).toLocaleString() : '0'}
          subtitle="Reads + Writes + Deletes"
          color="blue"
        />
        
        <MetricCard
          icon={<Zap className="w-6 h-6 text-green-600" />}
          title="Cache Hit Rate"
          value={`${cacheHitRate}%`}
          subtitle={`${metrics?.cacheHits.toLocaleString() || 0} hits`}
          color="green"
        />
        
        <MetricCard
          icon={<Clock className="w-6 h-6 text-purple-600" />}
          title="Avg Read Latency"
          value={`${metrics?.avgReadLatency.toFixed(2) || 0}ms`}
          subtitle="Average response time"
          color="purple"
        />
        
        <MetricCard
          icon={<Activity className="w-6 h-6 text-orange-600" />}
          title="Error Rate"
          value={`${errorRate}%`}
          subtitle={`${metrics?.graphqlErrors || 0} errors`}
          color="orange"
        />
      </div>

      {/* Detailed Metrics */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Storage Operations */}
        <MetricPanel title="Storage Operations" icon={<HardDrive className="w-5 h-5" />}>
          <div className="space-y-3">
            <MetricRow
              label="Reads"
              value={metrics?.storageReads.toLocaleString() || '0'}
              color="blue"
              percentage={metrics && (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) > 0 
                ? (metrics.storageReads / (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) * 100).toFixed(0)
                : '0'}
            />
            <MetricRow
              label="Writes"
              value={metrics?.storageWrites.toLocaleString() || '0'}
              color="green"
              percentage={metrics && (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) > 0
                ? (metrics.storageWrites / (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) * 100).toFixed(0)
                : '0'}
            />
            <MetricRow
              label="Deletes"
              value={metrics?.storageDeletes.toLocaleString() || '0'}
              color="red"
              percentage={metrics && (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) > 0
                ? (metrics.storageDeletes / (metrics.storageReads + metrics.storageWrites + metrics.storageDeletes) * 100).toFixed(0)
                : '0'}
            />
          </div>
        </MetricPanel>

        {/* Cache Performance */}
        <MetricPanel title="Cache Performance" icon={<Cpu className="w-5 h-5" />}>
          <div className="space-y-3">
            <MetricRow
              label="Hot Tier Hits"
              value={metrics?.cacheHotHits.toLocaleString() || '0'}
              color="orange"
              badge={`${metrics?.cacheSizeHot.toLocaleString() || 0} entries`}
            />
            <MetricRow
              label="Warm Tier Hits"
              value={metrics?.cacheWarmHits.toLocaleString() || '0'}
              color="yellow"
              badge={`${metrics?.cacheSizeWarm.toLocaleString() || 0} entries`}
            />
            <MetricRow
              label="Cache Misses"
              value={metrics?.cacheMisses.toLocaleString() || '0'}
              color="gray"
              percentage={metrics && (metrics.cacheHits + metrics.cacheMisses) > 0
                ? ((metrics.cacheMisses / (metrics.cacheHits + metrics.cacheMisses)) * 100).toFixed(1)
                : '0'}
            />
          </div>
        </MetricPanel>

        {/* Latency Stats */}
        <MetricPanel title="Operation Latency" icon={<Clock className="w-5 h-5" />}>
          <div className="space-y-3">
            <LatencyBar
              label="Read"
              value={metrics?.avgReadLatency || 0}
              color="blue"
            />
            <LatencyBar
              label="Write"
              value={metrics?.avgWriteLatency || 0}
              color="green"
            />
            <LatencyBar
              label="Delete"
              value={metrics?.avgDeleteLatency || 0}
              color="red"
            />
          </div>
        </MetricPanel>

        {/* GraphQL Stats */}
        <MetricPanel title="GraphQL Operations" icon={<Server className="w-5 h-5" />}>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-600 dark:text-gray-400">Total Requests</span>
              <span className="text-2xl font-bold text-gray-900 dark:text-white">
                {metrics?.graphqlRequests.toLocaleString() || '0'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-600 dark:text-gray-400">Errors</span>
              <span className="text-2xl font-bold text-red-600 dark:text-red-400">
                {metrics?.graphqlErrors.toLocaleString() || '0'}
              </span>
            </div>
            <div className="pt-3 border-t border-gray-200 dark:border-gray-700">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">Success Rate</span>
                <span className="text-lg font-bold text-green-600 dark:text-green-400">
                  {metrics && metrics.graphqlRequests > 0
                    ? ((1 - (metrics.graphqlErrors / metrics.graphqlRequests)) * 100).toFixed(2)
                    : '100'}%
                </span>
              </div>
            </div>
          </div>
        </MetricPanel>
      </div>

      {/* Simple Chart Visualization */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div className="flex items-center gap-2 mb-4">
          <BarChart3 className="w-5 h-5 text-gray-700 dark:text-gray-300" />
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white">Historical Trends</h2>
        </div>
        <div className="space-y-6">
          <SimpleChart
            data={history}
            dataKey="reads"
            label="Storage Reads"
            color="#3b82f6"
          />
          <SimpleChart
            data={history}
            dataKey="cacheHitRate"
            label="Cache Hit Rate (%)"
            color="#10b981"
          />
        </div>
      </div>
    </div>
  );
}

// Helper Components
interface MetricCardProps {
  icon: React.ReactNode;
  title: string;
  value: string;
  subtitle: string;
  color: 'blue' | 'green' | 'purple' | 'orange';
}

function MetricCard({ icon, title, value, subtitle, color }: MetricCardProps) {
  const colors = {
    blue: 'bg-blue-50 dark:bg-blue-900/20',
    green: 'bg-green-50 dark:bg-green-900/20',
    purple: 'bg-purple-50 dark:bg-purple-900/20',
    orange: 'bg-orange-50 dark:bg-orange-900/20',
  };

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className={`inline-flex p-3 rounded-lg ${colors[color]} mb-4`}>
        {icon}
      </div>
      <h3 className="text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">{title}</h3>
      <p className="text-3xl font-bold text-gray-900 dark:text-white mb-1">{value}</p>
      <p className="text-xs text-gray-500 dark:text-gray-500">{subtitle}</p>
    </div>
  );
}

interface MetricPanelProps {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}

function MetricPanel({ title, icon, children }: MetricPanelProps) {
  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className="flex items-center gap-2 mb-4">
        {icon}
        <h2 className="text-lg font-semibold text-gray-900 dark:text-white">{title}</h2>
      </div>
      {children}
    </div>
  );
}

interface MetricRowProps {
  label: string;
  value: string;
  color: string;
  percentage?: string;
  badge?: string;
}

function MetricRow({ label, value, color, percentage, badge }: MetricRowProps) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-3">
        <div className={`w-3 h-3 rounded-full bg-${color}-500`}></div>
        <span className="text-sm text-gray-700 dark:text-gray-300">{label}</span>
      </div>
      <div className="flex items-center gap-3">
        {badge && (
          <span className="text-xs px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-gray-600 dark:text-gray-400">
            {badge}
          </span>
        )}
        <span className="text-sm font-semibold text-gray-900 dark:text-white">{value}</span>
        {percentage && (
          <span className="text-xs text-gray-500 dark:text-gray-500">({percentage}%)</span>
        )}
      </div>
    </div>
  );
}

interface LatencyBarProps {
  label: string;
  value: number;
  color: string;
}

function LatencyBar({ label, value, color }: LatencyBarProps) {
  const percentage = Math.min((value / 50) * 100, 100); // Max 50ms for scale

  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <span className="text-sm text-gray-700 dark:text-gray-300">{label}</span>
        <span className="text-sm font-semibold text-gray-900 dark:text-white">{value.toFixed(2)}ms</span>
      </div>
      <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div
          className={`bg-${color}-500 h-2 rounded-full transition-all duration-300`}
          style={{ width: `${percentage}%` }}
        ></div>
      </div>
    </div>
  );
}

interface SimpleChartProps {
  data: HistoricalData[];
  dataKey: keyof HistoricalData;
  label: string;
  color: string;
}

function SimpleChart({ data, dataKey, label, color }: SimpleChartProps) {
  if (data.length === 0) return null;

  const values = data.map(d => d[dataKey] as number);
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const range = max - min || 1;

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{label}</span>
        <span className="text-sm text-gray-500 dark:text-gray-500">
          {data.length > 0 ? (data[data.length - 1][dataKey] as number).toFixed(2) : '0'}
        </span>
      </div>
      <div className="flex items-end gap-1 h-20">
        {data.map((point, i) => {
          const value = point[dataKey] as number;
          const height = ((value - min) / range) * 100;
          return (
            <div
              key={i}
              className="flex-1 bg-gradient-to-t rounded-t transition-all duration-300"
              style={{
                backgroundColor: color,
                height: `${height}%`,
                minHeight: '2px',
                opacity: 0.7 + (i / data.length) * 0.3,
              }}
            ></div>
          );
        })}
      </div>
    </div>
  );
}
