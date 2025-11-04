import { useEffect, useState } from 'react';
import { getActiveNodes } from '../services/pact-services';
import type { NodeInfo } from '../services/pact-services';
import { Search, RefreshCw, Eye, Activity, Clock, Database, Filter } from 'lucide-react';
import { useTheme } from '../context/ThemeContext';

export default function AllNodes({ onNodeClick }: { onNodeClick?: (peerId: string) => void }) {
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [filteredNodes, setFilteredNodes] = useState<NodeInfo[]>([]);
  const [searchText, setSearchText] = useState('');
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<'all' | 'active' | 'inactive'>('all');
  const { theme } = useTheme();
  const isDark = theme === 'dark';

  const loadNodes = () => {
    setLoading(true);
    getActiveNodes()
      .then((data) => {
        setNodes(data);
        setFilteredNodes(data);
        setLoading(false);
      })
      .catch((error) => {
        console.error('Error fetching nodes:', error);
        setLoading(false);
      });
  };

  useEffect(() => {
    loadNodes();
  }, []);

  useEffect(() => {
    let filtered = nodes;

    // Apply search filter
    if (searchText) {
      filtered = filtered.filter(
        (node) =>
          node.peer_id?.toLowerCase().includes(searchText.toLowerCase()) ||
          node.multiaddr?.toLowerCase().includes(searchText.toLowerCase()) ||
          node.status?.toLowerCase().includes(searchText.toLowerCase())
      );
    }

    // Apply status filter
    if (statusFilter !== 'all') {
      filtered = filtered.filter((node) =>
        statusFilter === 'active' ? node.status === 'active' : node.status !== 'active'
      );
    }

    setFilteredNodes(filtered);
  }, [nodes, searchText, statusFilter]);

  const activeNodesCount = nodes.filter((n) => n.status === 'active').length;
  const activePercentage = nodes.length > 0 ? Math.round((activeNodesCount / nodes.length) * 100) : 0;

  const getStatusBadge = (status: string) => {
    const isActive = status === 'active' || status === 'online';
    return (
      <span
        className={`inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs font-semibold ${
          isActive
            ? 'bg-green-500/20 text-green-500'
            : 'bg-gray-500/20 text-gray-500'
        }`}
      >
        {isActive ? <Activity className="h-3 w-3" /> : <Clock className="h-3 w-3" />}
        {status}
      </span>
    );
  };

  const extractIP = (multiaddr: string) => {
    const match = multiaddr?.match(/\/ip4\/([^/]+)/);
    return match ? match[1] : 'N/A';
  };

  return (
    <div className="space-y-8 p-6">
      {/* Header */}
      <div
        className={`rounded-2xl border p-8 shadow-xl animate-gradient ${
          isDark
            ? 'border-gray-700 bg-gradient-to-r from-blue-900/50 via-teal-900/50 to-cyan-900/50'
            : 'border-gray-200 bg-gradient-to-r from-blue-100 via-teal-100 to-cyan-100'
        }`}
      >
        <div className="flex items-center justify-between">
          <div>
            <h2 className={`mb-3 text-4xl font-bold gradient-text-blue`}>
              Network Nodes
            </h2>
            <p className={`text-lg ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
              Browse and monitor all active nodes in the Cyberfly network
            </p>
          </div>
                    <button
            onClick={loadNodes}
            disabled={loading}
            className={`flex items-center gap-2 rounded-xl px-6 py-3 font-semibold transition-all duration-300 transform hover:scale-105 shadow-lg hover:shadow-xl ${
              isDark
                ? 'bg-gradient-to-r from-blue-600 to-blue-700 text-white hover:from-blue-700 hover:to-blue-800 disabled:from-gray-700 disabled:to-gray-800'
                : 'bg-gradient-to-r from-blue-500 to-blue-600 text-white hover:from-blue-600 hover:to-blue-700 disabled:from-gray-300 disabled:to-gray-400'
            }`}
          >
            <RefreshCw className={`h-5 w-5 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
        </div>
      </div>

      {/* Statistics */}
      <div className="grid grid-cols-1 gap-6 sm:grid-cols-3">
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
            {nodes.length}
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
            {activeNodesCount}
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Active Nodes
          </div>
        </div>

        <div
          className={`rounded-xl border p-6 text-center card-hover shadow-lg ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-teal-900/50 to-teal-800/50'
              : 'border-gray-200 bg-gradient-to-br from-teal-50 to-teal-100'
          }`}
        >
          <div className="mb-3 flex justify-center">
            <div className="p-3 bg-teal-500 bg-opacity-20 rounded-full">
              <Activity className="w-8 h-8 text-teal-500" />
            </div>
          </div>
          <div className={`text-4xl font-bold ${isDark ? 'text-teal-400' : 'text-teal-600'}`}>
            {activePercentage}%
          </div>
          <div className={`mt-2 text-sm font-medium ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Health Rate
          </div>
        </div>
      </div>

      {/* Search and Filter */}
      <div
        className={`rounded-xl border p-6 shadow-lg ${
          isDark ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-900' : 'border-gray-200 bg-gradient-to-br from-white to-gray-50'
        }`}
      >
        <div className="flex flex-col gap-4 sm:flex-row">
          <div className="relative flex-1">
            <Search
              className={`absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 ${
                isDark ? 'text-gray-500' : 'text-gray-400'
              }`}
            />
            <input
              type="text"
              placeholder="Search by Peer ID, IP address, or status..."
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
              className={`w-full rounded-xl border py-3 pl-12 pr-4 text-base font-medium transition-all focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent ${
                isDark
                  ? 'border-gray-600 bg-gray-700 text-white placeholder-gray-400'
                  : 'border-gray-300 bg-white text-gray-900 placeholder-gray-500'
              }`}
            />
          </div>

          <div className="flex items-center gap-3">
            <div className="p-2 bg-blue-500 bg-opacity-10 rounded-lg">
              <Filter className={`h-5 w-5 text-blue-500`} />
            </div>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value as 'all' | 'active' | 'inactive')}
              className={`rounded-xl border px-5 py-3 font-medium transition-all focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent ${
                isDark
                  ? 'border-gray-600 bg-gray-700 text-white'
                  : 'border-gray-300 bg-white text-gray-900'
              }`}
            >
              <option value="all">All Status</option>
              <option value="active">Active Only</option>
              <option value="inactive">Inactive Only</option>
            </select>
          </div>
        </div>
      </div>

      {/* Nodes List */}
      {loading ? (
        <div
          className={`rounded-lg border p-8 text-center ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <div className="mx-auto mb-4 h-12 w-12 animate-spin rounded-full border-4 border-blue-500 border-t-transparent"></div>
          <p className={`font-medium ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
            Loading network nodes...
          </p>
        </div>
      ) : filteredNodes.length > 0 ? (
        <div
          className={`overflow-hidden rounded-lg border ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead
                className={isDark ? 'bg-gray-900 text-gray-300' : 'bg-gray-50 text-gray-700'}
              >
                <tr>
                  <th className="px-6 py-3 text-left text-sm font-semibold">Node Info</th>
                  <th className="px-6 py-3 text-left text-sm font-semibold">Status</th>
                  <th className="px-6 py-3 text-left text-sm font-semibold">IP Address</th>
                  <th className="px-6 py-3 text-left text-sm font-semibold">Actions</th>
                </tr>
              </thead>
              <tbody className={`divide-y ${isDark ? 'divide-gray-700' : 'divide-gray-200'}`}>
                {filteredNodes.map((node, index) => (
                  <tr
                    key={node.peer_id}
                    className={`transition-colors ${
                      isDark ? 'hover:bg-gray-700' : 'hover:bg-gray-50'
                    }`}
                  >
                    <td className="px-6 py-4">
                      <div className="flex items-center gap-3">
                        <div
                          className={`flex h-10 w-10 items-center justify-center rounded-full ${
                            node.status === 'active'
                              ? 'bg-green-500/20'
                              : 'bg-gray-500/20'
                          }`}
                        >
                          <Database
                            className={`h-5 w-5 ${
                              node.status === 'active' ? 'text-green-500' : 'text-gray-500'
                            }`}
                          />
                        </div>
                        <div>
                          <div
                            className={`font-mono text-sm font-medium ${
                              isDark ? 'text-white' : 'text-gray-900'
                            }`}
                          >
                            {node.peer_id.slice(0, 16)}...
                          </div>
                          <div
                            className={`text-xs ${isDark ? 'text-gray-500' : 'text-gray-500'}`}
                          >
                            Node #{index + 1}
                          </div>
                        </div>
                      </div>
                    </td>
                    <td className="px-6 py-4">{getStatusBadge(node.status)}</td>
                    <td className="px-6 py-4">
                      <div
                        className={`font-mono text-sm ${
                          isDark ? 'text-gray-400' : 'text-gray-600'
                        }`}
                      >
                        {extractIP(node.multiaddr)}
                      </div>
                    </td>
                    <td className="px-6 py-4">
                      <button
                        onClick={() => onNodeClick?.(node.peer_id)}
                        className={`inline-flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
                          isDark
                            ? 'bg-blue-600 text-white hover:bg-blue-700'
                            : 'bg-blue-500 text-white hover:bg-blue-600'
                        }`}
                      >
                        <Eye className="h-4 w-4" />
                        View Details
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <div
          className={`rounded-lg border p-8 text-center ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <Database
            className={`mx-auto mb-4 h-16 w-16 ${isDark ? 'text-gray-600' : 'text-gray-400'}`}
          />
          <h3 className={`mb-2 text-xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            No nodes found
          </h3>
          <p className={isDark ? 'text-gray-400' : 'text-gray-600'}>
            {searchText
              ? 'Try adjusting your search or filter criteria'
              : 'No nodes are currently available'}
          </p>
        </div>
      )}
    </div>
  );
}
