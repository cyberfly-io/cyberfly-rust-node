import { useQuery } from '@tanstack/react-query';
import { Activity, Database, Network, HardDrive, Copy, Check, Server, Clock, TrendingUp, Coins } from 'lucide-react';
import { getNodeInfo, getDiscoveredPeers } from '../api/client';
import { getAPY, getStakeStats } from '../services/kadena';
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

  const { data: apy } = useQuery({
    queryKey: ['apy'],
    queryFn: getAPY,
    refetchInterval: 60000, // Refetch every minute
  });

  const { data: stakeStats } = useQuery({
    queryKey: ['stakeStats'],
    queryFn: getStakeStats,
    refetchInterval: 30000, // Refetch every 30 seconds
  });

  return (
    <div className="p-6 space-y-8">
      <div>
        <h1 className="text-4xl font-bold gradient-text-blue mb-2">CyberFly Node Dashboard</h1>
        <p className="text-gray-600 dark:text-gray-400 text-lg">Monitor your decentralized network node</p>
      </div>

      {/* Node Info */}
      <div className="glass dark:glass-dark rounded-2xl shadow-2xl overflow-hidden card-hover backdrop-blur-xl border border-white/20 dark:border-gray-700/50">
        <div className="bg-gradient-to-r from-blue-500 via-blue-600 to-purple-600 px-8 py-6 animate-gradient">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div className="bg-white/20 p-3 rounded-xl shadow-lg backdrop-blur-sm">
                <Server className="w-8 h-8 text-white" />
              </div>
              <div>
                <h2 className="text-2xl font-bold text-white">Node Information</h2>
                <p className="text-blue-100 text-base">Decentralized Network Node</p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <div className={`px-4 py-2 rounded-xl text-sm font-bold shadow-lg backdrop-blur-sm ${
                nodeInfo?.health === 'healthy' 
                  ? 'bg-green-500/90 text-white' 
                  : nodeInfo?.health === 'discovering'
                  ? 'bg-yellow-500/90 text-white'
                  : 'bg-red-500/90 text-white'
              }`}>
                {nodeInfo?.health?.toUpperCase() || 'UNKNOWN'}
              </div>
            </div>
          </div>
        </div>
        
        <div className="p-6 space-y-6 backdrop-blur-xl">
          {/* Node IDs Section */}
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 dark:text-gray-300 uppercase tracking-wide">
              <div className="w-1 h-4 bg-blue-500 rounded"></div>
              Node Identifiers
            </div>
            
            <div className="glass dark:glass-dark rounded-lg p-4 backdrop-blur-md border border-white/10 dark:border-gray-600/30">
              <CopyableField 
                label="Node ID / Peer ID" 
                value={nodeInfo?.nodeId || 'Loading...'} 
                fullWidth
              />
            </div>
          </div>

          {/* Stats Grid */}
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 dark:text-gray-300 uppercase tracking-wide">
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
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-700 dark:text-gray-300 uppercase tracking-wide">
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

      {/* Staking Statistics */}
      <div className="glass dark:glass-dark rounded-2xl shadow-2xl overflow-hidden card-hover backdrop-blur-xl border border-white/20 dark:border-gray-700/50">
        <div className="bg-gradient-to-r from-green-500 via-emerald-600 to-teal-600 px-8 py-6 animate-gradient">
          <div className="flex items-center gap-4">
            <div className="bg-white/20 p-3 rounded-xl shadow-lg backdrop-blur-sm">
              <Coins className="w-8 h-8 text-white" />
            </div>
            <div>
              <h2 className="text-2xl font-bold text-white">Staking & Rewards</h2>
              <p className="text-green-100 text-base">Node rewards and staking information</p>
            </div>
          </div>
        </div>
        
                
        <div className="p-6 space-y-6 backdrop-blur-xl">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <StatBox
              icon={<TrendingUp className="w-5 h-5 text-green-600" />}
              label="Current APY"
              value={apy !== null && apy !== undefined ? `${apy.toFixed(2)}%` : 'Loading...'}
              subtitle="Annual percentage yield"
              color="green"
            />
            <StatBox
              icon={<Coins className="w-5 h-5 text-emerald-600" />}
              label="Active Stakes"
              value={stakeStats?.totalStakes !== undefined ? stakeStats.totalStakes.toString() : 'Loading...'}
              subtitle={stakeStats?.activeStakes !== undefined ? `Active: ${stakeStats.activeStakes} nodes` : 'Loading...'}
              color="green"
            />
            <StatBox
              icon={<Coins className="w-5 h-5 text-blue-600" />}
              label="Total Staked"
              value={stakeStats?.totalStakedAmount !== undefined ? `${stakeStats.totalStakedAmount.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} CFLY` : 'Loading...'}
              subtitle="Total amount staked"
              color="blue"
            />
          </div>
          
          <div className="glass dark:glass-dark rounded-lg p-4 backdrop-blur-md border border-white/10 dark:border-gray-600/30">
            <p className="text-sm text-gray-600 dark:text-gray-400 text-center">
              {apy !== null ? (
                `📊 Real-time data from Kadena blockchain`
              ) : (
                `💡 Connecting to Kadena network...`
              )}
            </p>
          </div>
        </div>
      </div>

      {/* Connected Peers */}
      <div className="glass dark:glass-dark rounded-2xl shadow-2xl p-6 backdrop-blur-xl border border-white/20 dark:border-gray-700/50">
        <h2 className="text-xl font-semibold text-gray-900 dark:text-white mb-4 flex items-center gap-2">
          <Network className="w-5 h-5 text-blue-500" />
          Connected Peers ({peers.length})
        </h2>
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {peers.length === 0 ? (
            <p className="text-gray-500 dark:text-gray-400 text-center py-8">No peers connected</p>
          ) : (
            peers.map((peer) => (
              <div
                key={peer.peerId}
                className="flex items-center justify-between p-3 glass dark:glass-dark rounded-lg hover:bg-white/30 dark:hover:bg-gray-700/50 transition backdrop-blur-md border border-white/10 dark:border-gray-600/30"
              >
                <div className="flex items-center gap-3">
                  <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse shadow-lg shadow-green-500/50"></div>
                  <code className="text-sm font-mono text-gray-700 dark:text-gray-300">{peer.peerId}</code>
                </div>
                <span className="text-xs text-gray-500 dark:text-gray-400">
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
      <label className="block text-xs font-medium text-gray-600 dark:text-gray-400 mb-2">{label}</label>
      <div className="flex items-center gap-2">
        <code className="flex-1 bg-white dark:bg-gray-700 px-3 py-2 rounded border border-gray-300 dark:border-gray-600 text-sm font-mono text-gray-800 dark:text-gray-200 overflow-x-auto">
          {value}
        </code>
        <button
          onClick={handleCopy}
          className="p-2 hover:bg-gray-200 dark:hover:bg-gray-600 rounded transition-colors flex-shrink-0"
          title="Copy to clipboard"
        >
          {copied ? (
            <Check className="w-4 h-4 text-green-600" />
          ) : (
            <Copy className="w-4 h-4 text-gray-600 dark:text-gray-400" />
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
    blue: 'glass dark:glass-dark border-blue-200/50 dark:border-blue-700/30 hover:border-blue-300 dark:hover:border-blue-600',
    green: 'glass dark:glass-dark border-green-200/50 dark:border-green-700/30 hover:border-green-300 dark:hover:border-green-600',
    purple: 'glass dark:glass-dark border-purple-200/50 dark:border-purple-700/30 hover:border-purple-300 dark:hover:border-purple-600',
    orange: 'glass dark:glass-dark border-orange-200/50 dark:border-orange-700/30 hover:border-orange-300 dark:hover:border-orange-600',
  };

  return (
    <div className={`${colors[color]} border rounded-xl p-4 backdrop-blur-md transition-all duration-300 hover:shadow-lg`}>
      <div className="flex items-center gap-2 mb-2">
        <div className="p-2 bg-white/30 dark:bg-gray-700/30 rounded-lg backdrop-blur-sm">
          {icon}
        </div>
        <span className="text-xs font-medium text-gray-600 dark:text-gray-400 uppercase tracking-wide">{label}</span>
      </div>
      <div className="text-2xl font-bold text-gray-900 dark:text-white mb-1">{value}</div>
      <div className="text-xs text-gray-500 dark:text-gray-400">{subtitle}</div>
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
    <div className="glass dark:glass-dark rounded-xl p-4 backdrop-blur-md border border-white/20 dark:border-gray-600/30 hover:border-white/30 dark:hover:border-gray-500/40 transition-all duration-300">
      <div className="text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">{label}</div>
      <div className={`text-sm font-medium ${muted ? 'text-gray-500 dark:text-gray-400 italic' : 'text-gray-900 dark:text-white'}`}>
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
