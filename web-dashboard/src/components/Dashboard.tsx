import { useQuery } from '@tanstack/react-query';
import { Activity, Database, Network, HardDrive } from 'lucide-react';
import { getNodeInfo, getConnectedPeers } from '../api/client';

export default function Dashboard() {
  const { data: nodeInfo } = useQuery({
    queryKey: ['nodeInfo'],
    queryFn: getNodeInfo,
    refetchInterval: 5000,
  });

  const { data: peers = [] } = useQuery({
    queryKey: ['peers'],
    queryFn: getConnectedPeers,
    refetchInterval: 5000,
  });

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold text-gray-900">CyberFly Node Dashboard</h1>
        <div className="flex items-center gap-2">
          <div className={`w-3 h-3 rounded-full animate-pulse ${
            nodeInfo?.health === 'healthy' ? 'bg-green-500' : 'bg-yellow-500'
          }`}></div>
          <span className="text-sm text-gray-600">{nodeInfo?.health || 'Unknown'}</span>
        </div>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <StatCard
          icon={<Network className="w-6 h-6" />}
          title="Connected Peers"
          value={nodeInfo?.connectedPeers || 0}
          subtitle={`${nodeInfo?.discoveredPeers || 0} discovered`}
          color="blue"
        />
        <StatCard
          icon={<Activity className="w-6 h-6" />}
          title="Node ID"
          value={nodeInfo?.nodeId?.slice(0, 8) || 'N/A'}
          subtitle="Identifier"
          color="green"
        />
        <StatCard
          icon={<Database className="w-6 h-6" />}
          title="Uptime"
          value={formatUptime(nodeInfo?.uptimeSeconds || 0)}
          subtitle="Running time"
          color="purple"
        />
        <StatCard
          icon={<HardDrive className="w-6 h-6" />}
          title="Active Peers"
          value={peers.length}
          subtitle="Currently connected"
          color="orange"
        />
      </div>

      {/* Node Info */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-xl font-semibold mb-4">Node Information</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <InfoRow label="Peer ID" value={nodeInfo?.peerId || 'Loading...'} mono />
          <InfoRow label="Node ID" value={nodeInfo?.nodeId || 'Loading...'} mono />
          <InfoRow label="Health" value={nodeInfo?.health || '-'} />
          <InfoRow label="Uptime" value={formatUptime(nodeInfo?.uptimeSeconds || 0)} />
          <InfoRow label="Connected Peers" value={nodeInfo?.connectedPeers || 0} />
          <InfoRow label="Relay URL" value={nodeInfo?.relayUrl || 'None'} />
        </div>
      </div>

      {/* Connected Peers */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-xl font-semibold mb-4">Connected Peers ({peers.length})</h2>
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {peers.length === 0 ? (
            <p className="text-gray-500 text-center py-8">No peers connected</p>
          ) : (
            peers.map((peer) => (
              <div
                key={peer.peerId}
                className="flex items-center justify-between p-3 bg-gray-50 rounded-lg hover:bg-gray-100 transition"
              >
                <div className="flex items-center gap-3">
                  <div className="w-2 h-2 bg-green-500 rounded-full"></div>
                  <code className="text-sm font-mono text-gray-700">{peer.peerId}</code>
                </div>
                <span className="text-xs text-gray-500">
                  {formatRelativeTime(peer.lastSeen)}
                </span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

interface StatCardProps {
  icon: React.ReactNode;
  title: string;
  value: string | number;
  subtitle: string;
  color: 'blue' | 'green' | 'purple' | 'orange';
}

function StatCard({ icon, title, value, subtitle, color }: StatCardProps) {
  const colors = {
    blue: 'bg-blue-50 text-blue-600',
    green: 'bg-green-50 text-green-600',
    purple: 'bg-purple-50 text-purple-600',
    orange: 'bg-orange-50 text-orange-600',
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className={`inline-flex p-3 rounded-lg ${colors[color]} mb-4`}>
        {icon}
      </div>
      <h3 className="text-sm font-medium text-gray-600 mb-1">{title}</h3>
      <p className="text-2xl font-bold text-gray-900 mb-1">{value}</p>
      <p className="text-xs text-gray-500">{subtitle}</p>
    </div>
  );
}

interface InfoRowProps {
  label: string;
  value: string | number;
  mono?: boolean;
}

function InfoRow({ label, value, mono }: InfoRowProps) {
  return (
    <div>
      <dt className="text-sm font-medium text-gray-500">{label}</dt>
      <dd className={`mt-1 text-sm text-gray-900 ${mono ? 'font-mono' : ''}`}>
        {value}
      </dd>
    </div>
  );
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function formatRelativeTime(timestamp: string): string {
  const now = Date.now();
  const then = new Date(timestamp).getTime();
  const diff = now - then;
  
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return 'just now';
  
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
