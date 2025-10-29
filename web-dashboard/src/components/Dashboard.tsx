import { useQuery } from '@tanstack/react-query';
import { Activity, Database, Network, HardDrive, Copy, Check, Server, Clock } from 'lucide-react';
import { getNodeInfo, getDiscoveredPeers } from '../api/client';
import { useState } from 'react';

export default function Dashboard() {
  const { data: nodeInfo } = useQuery({
    queryKey: ['nodeInfo'],
    queryFn: getNodeInfo,
    refetchInterval: 5000,
  });

  const { data: peers = [] } = useQuery({
    queryKey: ['peers'],
    queryFn: getDiscoveredPeers,
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

   

      {/* Node Info */}
      <div className="bg-white rounded-lg shadow-md overflow-hidden">
        <div className="bg-gradient-to-r from-blue-500 to-blue-600 px-6 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="bg-white/20 p-2 rounded-lg">
                <Server className="w-6 h-6 text-white" />
              </div>
              <div>
                <h2 className="text-xl font-semibold text-white">Node Information</h2>
                <p className="text-blue-100 text-sm">Decentralized Network Node</p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <div className={`px-3 py-1 rounded-full text-xs font-medium ${
                nodeInfo?.health === 'healthy' 
                  ? 'bg-green-500 text-white' 
                  : nodeInfo?.health === 'discovering'
                  ? 'bg-yellow-500 text-white'
                  : 'bg-red-500 text-white'
              }`}>
                {nodeInfo?.health?.toUpperCase() || 'UNKNOWN'}
              </div>
            </div>
          </div>
        </div>
        
        <div className="p-6 space-y-6">
          {/* Node IDs Section */}
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 uppercase tracking-wide">
              <div className="w-1 h-4 bg-blue-500 rounded"></div>
              Node Identifiers
            </div>
            
            <div className="bg-gradient-to-br from-gray-50 to-gray-100 rounded-lg p-4 border border-gray-200">
              <CopyableField 
                label="Node ID / Peer ID" 
                value={nodeInfo?.nodeId || 'Loading...'} 
                fullWidth
              />
            </div>
          </div>

          {/* Stats Grid */}
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 uppercase tracking-wide">
              <div className="w-1 h-4 bg-green-500 rounded"></div>
              Network Statistics
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <StatBox
                icon={<Network className="w-5 h-5 text-blue-600" />}
                label="Connected Peers"
                value={nodeInfo?.connectedPeers || 0}
                subtitle="Active connections"
                color="blue"
              />
              <StatBox
                icon={<Activity className="w-5 h-5 text-green-600" />}
                label="Discovered Peers"
                value={nodeInfo?.discoveredPeers || 0}
                subtitle="Network participants"
                color="green"
              />
              <StatBox
                icon={<Clock className="w-5 h-5 text-purple-600" />}
                label="Uptime"
                value={formatUptime(nodeInfo?.uptimeSeconds || 0)}
                subtitle={`${nodeInfo?.uptimeSeconds || 0} seconds`}
                color="purple"
              />
            </div>
          </div>

          {/* Additional Info */}
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 uppercase tracking-wide">
              <div className="w-1 h-4 bg-purple-500 rounded"></div>
              Configuration
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <InfoBox 
                label="Relay URL" 
                value={nodeInfo?.relayUrl || 'Not configured'} 
                muted={!nodeInfo?.relayUrl}
              />
              <InfoBox 
                label="Protocol Version" 
                value="Iroh v0.94.0" 
              />
            </div>
          </div>
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

// New components for improved UI
interface CopyableFieldProps {
  label: string;
  value: string;
  fullWidth?: boolean;
}

function CopyableField({ label, value, fullWidth }: CopyableFieldProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className={fullWidth ? 'w-full' : ''}>
      <label className="block text-xs font-medium text-gray-600 mb-2">{label}</label>
      <div className="flex items-center gap-2">
        <code className="flex-1 bg-white px-3 py-2 rounded border border-gray-300 text-sm font-mono text-gray-800 overflow-x-auto">
          {value}
        </code>
        <button
          onClick={handleCopy}
          className="p-2 hover:bg-gray-200 rounded transition-colors flex-shrink-0"
          title="Copy to clipboard"
        >
          {copied ? (
            <Check className="w-4 h-4 text-green-600" />
          ) : (
            <Copy className="w-4 h-4 text-gray-600" />
          )}
        </button>
      </div>
    </div>
  );
}

interface StatBoxProps {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  subtitle: string;
  color: 'blue' | 'green' | 'purple' | 'orange';
}

function StatBox({ icon, label, value, subtitle, color }: StatBoxProps) {
  const colors = {
    blue: 'bg-blue-50 border-blue-200',
    green: 'bg-green-50 border-green-200',
    purple: 'bg-purple-50 border-purple-200',
    orange: 'bg-orange-50 border-orange-200',
  };

  return (
    <div className={`${colors[color]} border rounded-lg p-4`}>
      <div className="flex items-center gap-2 mb-2">
        {icon}
        <span className="text-xs font-medium text-gray-600 uppercase tracking-wide">{label}</span>
      </div>
      <div className="text-2xl font-bold text-gray-900 mb-1">{value}</div>
      <div className="text-xs text-gray-500">{subtitle}</div>
    </div>
  );
}

interface InfoBoxProps {
  label: string;
  value: string;
  muted?: boolean;
}

function InfoBox({ label, value, muted }: InfoBoxProps) {
  return (
    <div className="bg-gray-50 rounded-lg p-4 border border-gray-200">
      <div className="text-xs font-medium text-gray-600 mb-1">{label}</div>
      <div className={`text-sm font-medium ${muted ? 'text-gray-400 italic' : 'text-gray-900'}`}>
        {value}
      </div>
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
