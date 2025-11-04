import { useEffect, useState } from 'react';
import { useKadenaWallet } from '../context/KadenaWalletContext';
import { getMyNodes, getNodeStake } from '../services/pact-services';
import type { NodeInfo, NodeStakeInfo } from '../services/pact-services';
import { Wallet, Eye, Activity, Clock, Zap, Database } from 'lucide-react';
import { useTheme } from '../context/ThemeContext';

export default function MyNodes({ onNodeClick }: { onNodeClick?: (peerId: string) => void }) {
  const [myNodes, setMyNodes] = useState<NodeInfo[]>([]);
  const [nodeStakes, setNodeStakes] = useState<Record<string, NodeStakeInfo>>({});
  const [loading, setLoading] = useState(true);
  const { account, initializeKadenaWallet } = useKadenaWallet();
  const { theme } = useTheme();
  const isDark = theme === 'dark';

  const fetchNodeStakes = async (nodes: NodeInfo[]) => {
    const stakes: Record<string, NodeStakeInfo> = {};
    await Promise.all(
      nodes.map(async (node) => {
        try {
          const stakeData = await getNodeStake(node.peer_id);
          stakes[node.peer_id] = stakeData;
        } catch (error) {
          console.error(`Error fetching stake for node ${node.peer_id}:`, error);
          stakes[node.peer_id] = { active: false };
        }
      })
    );
    setNodeStakes(stakes);
  };

  useEffect(() => {
    if (account) {
      setLoading(true);
      getMyNodes(account)
        .then((data) => {
          setMyNodes(data);
          fetchNodeStakes(data).finally(() => {
            setLoading(false);
          });
        })
        .catch((error) => {
          console.error('Error fetching nodes:', error);
          setLoading(false);
        });
    } else {
      setLoading(false);
    }
  }, [account]);

  // Calculate statistics
  const totalNodes = myNodes.length;
  const activeNodes = myNodes.filter((node) => node.status === 'active').length;
  const inactiveNodes = totalNodes - activeNodes;
  const stakedNodes = Object.values(nodeStakes).filter((stake) => stake?.active).length;

  const renderNodeCard = (node: NodeInfo) => {
    const stakeInfo = nodeStakes[node.peer_id];
    const isActive = node.status === 'active';
    const isStaked = stakeInfo?.active;

    return (
      <div
        key={node.peer_id}
        className={`group rounded-xl border p-6 transition-all duration-300 hover:shadow-2xl card-hover cursor-pointer ${
          isDark
            ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-900 hover:border-blue-500'
            : 'border-gray-200 bg-gradient-to-br from-white to-gray-50 hover:border-blue-400'
        }`}
      >
        <div className="mb-4 flex items-center justify-between">
          <h3 className={`text-lg font-bold group-hover:text-blue-500 transition-colors ${isDark ? 'text-white' : 'text-gray-900'}`}>
            Node {node.peer_id.slice(0, 8)}...
          </h3>
          <div className="flex gap-2">
            {isActive && (
              <span className="flex items-center gap-1 rounded-full bg-gradient-to-r from-green-400 to-green-600 px-3 py-1 text-xs font-semibold text-white shadow-lg">
                <Activity className="h-3 w-3 animate-pulse" />
                Active
              </span>
            )}
            {isStaked && (
              <span className="flex items-center gap-1 rounded-full bg-gradient-to-r from-yellow-400 to-orange-500 px-3 py-1 text-xs font-semibold text-white shadow-lg">
                <Zap className="h-3 w-3" />
                Staked
              </span>
            )}
          </div>
        </div>

        <div className={`mb-4 space-y-3 text-sm ${isDark ? 'text-gray-300' : 'text-gray-600'}`}>
          <div className="flex items-start gap-3 p-3 rounded-lg bg-opacity-50 ${isDark ? 'bg-gray-700' : 'bg-gray-100'}">
            <div className="p-2 bg-blue-500 bg-opacity-10 rounded-lg">
              <Database className="h-4 w-4 text-blue-500" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="font-semibold mb-1">Peer ID</div>
              <div className="truncate font-mono text-xs bg-black bg-opacity-20 px-2 py-1 rounded">{node.peer_id}</div>
            </div>
          </div>

          <div className="flex items-center gap-3 p-3 rounded-lg ${isDark ? 'bg-gray-700 bg-opacity-50' : 'bg-gray-100'}">
            <div className={`p-2 rounded-lg ${isActive ? 'bg-green-500 bg-opacity-10' : 'bg-gray-500 bg-opacity-10'}`}>
              <Clock className={`h-4 w-4 ${isActive ? 'text-green-500' : 'text-gray-500'}`} />
            </div>
            <div>
              <span className="font-semibold">Status:</span>{' '}
              <span className={isActive ? 'text-green-500 font-bold' : 'text-gray-500'}>
                {node.status}
              </span>
            </div>
          </div>

          {isStaked && stakeInfo && (
            <div className="flex items-center gap-3 p-3 rounded-lg bg-gradient-to-r from-yellow-500 from-opacity-10 to-orange-500 to-opacity-10">
              <div className="p-2 bg-yellow-500 bg-opacity-20 rounded-lg">
                <Zap className="h-4 w-4 text-yellow-500" />
              </div>
              <div>
                <span className="font-semibold">Staked:</span>{' '}
                <span className="text-orange-500 font-bold">{stakeInfo.amount?.toLocaleString() || '50,000'} CFLY</span>
              </div>
            </div>
          )}
        </div>

        <button
          onClick={() => onNodeClick?.(node.peer_id)}
          className={`flex w-full items-center justify-center gap-2 rounded-lg px-4 py-3 font-semibold transition-all duration-200 transform hover:scale-105 shadow-md hover:shadow-lg ${
            isDark
              ? 'bg-gradient-to-r from-blue-600 to-blue-700 text-white hover:from-blue-700 hover:to-blue-800'
              : 'bg-gradient-to-r from-blue-500 to-blue-600 text-white hover:from-blue-600 hover:to-blue-700'
          }`}
        >
          <Eye className="h-4 w-4" />
          View Details
        </button>
      </div>
    );
  };

  if (!account) {
    return (
      <div
        className={`rounded-lg border p-8 text-center ${
          isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
        }`}
      >
        <Wallet className={`mx-auto mb-4 h-16 w-16 ${isDark ? 'text-gray-600' : 'text-gray-400'}`} />
        <h3 className={`mb-2 text-xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
          Connect Your Wallet
        </h3>
        <p className={`mb-4 ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
          Connect your Kadena wallet to view and manage your nodes
        </p>
        <button
          onClick={() => initializeKadenaWallet('eckoWallet')}
          className="inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-blue-500 to-purple-600 px-6 py-3 font-semibold text-white transition-transform hover:scale-105"
        >
          <Wallet className="h-5 w-5" />
          Connect Wallet
        </button>
      </div>
    );
  }

  if (loading) {
    return (
      <div
        className={`rounded-lg border p-8 text-center ${
          isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
        }`}
      >
        <div className="mx-auto mb-4 h-12 w-12 animate-spin rounded-full border-4 border-blue-500 border-t-transparent"></div>
        <p className={`font-medium ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
          Loading your nodes...
        </p>
        <p className={`mt-2 text-sm ${isDark ? 'text-gray-500' : 'text-gray-500'}`}>
          Fetching node data and staking information
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-8 p-6">
      {/* Header */}
      <div
        className={`rounded-2xl border p-8 shadow-xl animate-gradient ${
          isDark
            ? 'border-gray-700 bg-gradient-to-r from-purple-900/50 via-blue-900/50 to-indigo-900/50'
            : 'border-gray-200 bg-gradient-to-r from-purple-100 via-blue-100 to-indigo-100'
        }`}
      >
        <h2 className={`mb-3 text-4xl font-bold gradient-text-blue`}>
          My Nodes
        </h2>
        <p className={`text-lg ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
          Manage and monitor your Cyberfly network nodes
        </p>
      </div>

      {/* Statistics */}
      <div className="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
        <div
          className={`rounded-xl border p-6 text-center card-hover shadow-lg ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-blue-900/50 to-blue-800/50'
              : 'border-gray-200 bg-gradient-to-br from-blue-50 to-blue-100'
          }`}
        >
          <div className="mb-3 flex justify-center">
            <div className="p-3 bg-blue-500 bg-opacity-20 rounded-full">
              <Database className="w-8 h-8 text-blue-500" />
            </div>
          </div>
          <div className={`text-4xl font-bold ${isDark ? 'text-blue-400' : 'text-blue-600'}`}>
            {totalNodes}
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Total Nodes
          </div>
        </div>

        <div
          className={`rounded-xl border p-6 text-center card-hover shadow-lg ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-green-900/50 to-green-800/50'
              : 'border-gray-200 bg-gradient-to-br from-green-50 to-green-100'
          }`}
        >
          <div className="mb-3 flex justify-center">
            <div className="p-3 bg-green-500 bg-opacity-20 rounded-full">
              <Activity className="w-8 h-8 text-green-500 animate-pulse" />
            </div>
          </div>
          <div className={`text-4xl font-bold ${isDark ? 'text-green-400' : 'text-green-600'}`}>
            {activeNodes}
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Active Nodes
          </div>
        </div>

        <div
          className={`rounded-xl border p-6 text-center card-hover shadow-lg ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-700'
              : 'border-gray-200 bg-gradient-to-br from-gray-50 to-gray-100'
          }`}
        >
          <div className="mb-3 flex justify-center">
            <div className="p-3 bg-gray-500 bg-opacity-20 rounded-full">
              <Clock className="w-8 h-8 text-gray-500" />
            </div>
          </div>
          <div className={`text-4xl font-bold ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            {inactiveNodes}
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Inactive Nodes
          </div>
        </div>

        <div
          className={`rounded-xl border p-6 text-center card-hover shadow-lg ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-orange-900/50 to-orange-800/50'
              : 'border-gray-200 bg-gradient-to-br from-orange-50 to-orange-100'
          }`}
        >
          <div className="mb-3 flex justify-center">
            <div className="p-3 bg-yellow-500 bg-opacity-20 rounded-full">
              <Zap className="w-8 h-8 text-yellow-500" />
            </div>
          </div>
          <div className={`text-4xl font-bold ${isDark ? 'text-orange-400' : 'text-orange-600'}`}>
            {stakedNodes}
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Staked Nodes
          </div>
        </div>
      </div>

      {/* Nodes Grid */}
      {myNodes.length > 0 ? (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          {myNodes.map(renderNodeCard)}
        </div>
      ) : (
        <div
          className={`rounded-lg border p-8 text-center ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <Database className={`mx-auto mb-4 h-16 w-16 ${isDark ? 'text-gray-600' : 'text-gray-400'}`} />
          <h3 className={`mb-2 text-xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            No nodes found
          </h3>
          <p className={isDark ? 'text-gray-400' : 'text-gray-600'}>
            You haven't registered any nodes yet
          </p>
        </div>
      )}
    </div>
  );
}
