import { useEffect, useState } from 'react';
import { useKadenaWallet } from '../context/KadenaWalletContext';
import {
  getNode,
  getNodeStake,
  getNodeClaimable,
  getAPY,
  nodeStake,
  nodeUnStake,
  claimReward,
} from '../services/pact-services';
import type { NodeInfo, NodeStakeInfo, ClaimableReward } from '../services/pact-services';
import { ArrowUp, ArrowDown, Gift, Wallet, Clock, Zap, Database, TrendingUp, Activity } from 'lucide-react';
import { useTheme } from '../context/ThemeContext';

interface NodeDetailsProps {
  peerId: string;
  onBack?: () => void;
}

export default function NodeDetails({ peerId, onBack }: NodeDetailsProps) {
  const [nodeInfo, setNodeInfo] = useState<NodeInfo | null>(null);
  const [nodeStakeInfo, setNodeStakeInfo] = useState<NodeStakeInfo | null>(null);
  const [claimable, setClaimable] = useState<ClaimableReward | null>(null);
  const [apy, setApy] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const { account, initializeKadenaWallet, showNotification } = useKadenaWallet();
  const { theme } = useTheme();
  const isDark = theme === 'dark';

  const loadNodeData = async () => {
    setLoading(true);
    try {
      const [node, stake, reward, apyData] = await Promise.all([
        getNode(peerId),
        getNodeStake(peerId),
        getNodeClaimable(peerId),
        getAPY(),
      ]);
      setNodeInfo(node);
      setNodeStakeInfo(stake);
      setClaimable(reward);
      setApy(apyData);
    } catch (error) {
      console.error('Error loading node data:', error);
      showNotification('Failed to load node information', 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadNodeData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [peerId]);

  const handleStake = async () => {
    if (!account) {
      initializeKadenaWallet('eckoWallet');
      return;
    }

    setActionLoading(true);
    try {
      await nodeStake(account, peerId);
      showNotification('Staking transaction submitted successfully!', 'success');
      setTimeout(loadNodeData, 2000); // Reload data after 2 seconds
    } catch (error) {
      showNotification((error as Error).message || 'Staking failed', 'error');
    } finally {
      setActionLoading(false);
    }
  };

  const handleUnstake = async () => {
    if (!account) {
      initializeKadenaWallet('eckoWallet');
      return;
    }

    setActionLoading(true);
    try {
      await nodeUnStake(account, peerId);
      showNotification('Unstaking transaction submitted successfully!', 'success');
      setTimeout(loadNodeData, 2000);
    } catch (error) {
      showNotification((error as Error).message || 'Unstaking failed', 'error');
    } finally {
      setActionLoading(false);
    }
  };

  const handleClaim = async () => {
    if (!account) {
      initializeKadenaWallet('eckoWallet');
      return;
    }

    setActionLoading(true);
    try {
      await claimReward(account, peerId);
      showNotification('Claim transaction submitted successfully!', 'success');
      setTimeout(loadNodeData, 2000);
    } catch (error) {
      showNotification((error as Error).message || 'Claim failed', 'error');
    } finally {
      setActionLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[60vh] p-6">
        <div
          className={`rounded-2xl border p-12 text-center shadow-2xl ${
            isDark ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-900' : 'border-gray-200 bg-gradient-to-br from-white to-gray-50'
          }`}
        >
          <div className="relative mb-6">
            <div className="animate-spin rounded-full h-20 w-20 border-4 border-blue-200 dark:border-blue-900 mx-auto"></div>
            <div className="animate-spin rounded-full h-20 w-20 border-t-4 border-blue-600 mx-auto absolute top-0"></div>
          </div>
          <p className={`text-lg font-medium ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
            Loading node information...
          </p>
        </div>
      </div>
    );
  }

  if (!nodeInfo) {
    return (
      <div className="flex items-center justify-center min-h-[60vh] p-6">
        <div
          className={`rounded-2xl border p-12 text-center max-w-md shadow-2xl ${
            isDark ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-900' : 'border-gray-200 bg-gradient-to-br from-white to-gray-50'
          }`}
        >
          <Database
            className={`mx-auto mb-6 h-20 w-20 ${isDark ? 'text-gray-600' : 'text-gray-400'} opacity-50`}
          />
          <h3 className={`mb-3 text-3xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            Node Not Found
          </h3>
          <p className={`mb-6 text-lg ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            This node could not be located in the network
          </p>
          {onBack && (
            <button
              onClick={onBack}
              className="rounded-xl bg-gradient-to-r from-blue-500 to-blue-600 px-8 py-3 font-semibold text-white hover:from-blue-600 hover:to-blue-700 transition-all duration-300 transform hover:scale-105 shadow-lg"
            >
              Go Back
            </button>
          )}
        </div>
      </div>
    );
  }

  const canStake = !nodeStakeInfo?.active;
  const isActive = nodeInfo.status === 'active';

  return (
    <div className="space-y-8 p-6">
      {/* Header */}
      <div
        className={`rounded-2xl border p-8 shadow-xl animate-gradient ${
          isDark
            ? 'border-gray-700 bg-gradient-to-r from-purple-900/50 via-pink-900/50 to-rose-900/50'
            : 'border-gray-200 bg-gradient-to-r from-purple-100 via-pink-100 to-rose-100'
        }`}
      >
        <div className="flex items-center justify-between">
          <div className="flex-1">
            <h2 className={`mb-3 text-4xl font-bold gradient-text-blue`}>
              Node Details
            </h2>
            <p className={`font-mono text-base break-all ${isDark ? 'text-gray-300' : 'text-gray-700'}`}>
              {peerId}
            </p>
          </div>
          {onBack && (
            <button
              onClick={onBack}
              className={`rounded-xl px-6 py-3 font-semibold transition-all duration-300 transform hover:scale-105 shadow-lg hover:shadow-xl ${
                isDark
                  ? 'bg-gradient-to-r from-gray-700 to-gray-800 text-gray-300 hover:from-gray-600 hover:to-gray-700'
                  : 'bg-gradient-to-r from-white to-gray-50 text-gray-700 hover:from-gray-50 hover:to-gray-100 border border-gray-200'
              }`}
            >
              ← Back
            </button>
          )}
        </div>
      </div>

      {/* Status Card */}
      <div
        className={`rounded-xl border p-6 shadow-lg card-hover ${
          isDark ? 'border-gray-700 bg-gradient-to-br from-gray-800 to-gray-900' : 'border-gray-200 bg-gradient-to-br from-white to-gray-50'
        }`}
      >
        <div className="mb-6 flex items-center gap-3">
          <div className={`p-3 rounded-xl ${isActive ? 'bg-green-500 bg-opacity-20' : 'bg-gray-500 bg-opacity-20'}`}>
            <Activity className={`h-6 w-6 ${isActive ? 'text-green-500 animate-pulse' : 'text-gray-500'}`} />
          </div>
          <h3 className={`text-2xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            Node Overview
          </h3>
          <span
            className={`ml-auto flex items-center gap-2 rounded-full px-4 py-2 text-sm font-bold shadow-md ${
              isActive
                ? 'bg-gradient-to-r from-green-400 to-green-600 text-white'
                : 'bg-gradient-to-r from-gray-400 to-gray-600 text-white'
            }`}
          >
            {isActive ? 'Online' : 'Offline'}
          </span>
        </div>

        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          <div className={`p-4 rounded-lg ${isDark ? 'bg-gray-700 bg-opacity-50' : 'bg-gray-100'}`}>
            <div className={`text-sm font-medium mb-2 ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>Status</div>
            <div className={`font-bold text-lg ${isDark ? 'text-white' : 'text-gray-900'}`}>
              {nodeInfo.status}
            </div>
          </div>

          <div className={`p-4 rounded-lg ${isDark ? 'bg-gray-700 bg-opacity-50' : 'bg-gray-100'}`}>
            <div className={`text-sm font-medium mb-2 ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>Account</div>
            <div
              className={`mt-1 truncate font-mono text-sm ${
                isDark ? 'text-white' : 'text-gray-900'
              }`}
            >
              {nodeInfo.account}
            </div>
          </div>

          <div className="md:col-span-2">
            <div className={`text-sm ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
              Multiaddr
            </div>
            <div
              className={`mt-1 break-all font-mono text-xs ${isDark ? 'text-white' : 'text-gray-900'}`}
            >
              {nodeInfo.multiaddr}
            </div>
          </div>
        </div>
      </div>

      {/* Statistics Grid */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <div
          className={`rounded-lg border p-4 text-center ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-orange-900/50 to-orange-800/50'
              : 'border-gray-200 bg-gradient-to-br from-orange-50 to-orange-100'
          }`}
        >
          <Zap className={`mx-auto mb-2 h-6 w-6 ${isDark ? 'text-orange-400' : 'text-orange-600'}`} />
          <div className={`text-sm ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Staking Status
          </div>
          <div className={`mt-1 text-xl font-bold ${isDark ? 'text-orange-400' : 'text-orange-600'}`}>
            {nodeStakeInfo?.active ? 'Staked' : 'Not Staked'}
          </div>
        </div>

        <div
          className={`rounded-lg border p-4 text-center ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-blue-900/50 to-blue-800/50'
              : 'border-gray-200 bg-gradient-to-br from-blue-50 to-blue-100'
          }`}
        >
          <Gift className={`mx-auto mb-2 h-6 w-6 ${isDark ? 'text-blue-400' : 'text-blue-600'}`} />
          <div className={`text-sm ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Claimable Rewards
          </div>
          <div className={`mt-1 text-xl font-bold ${isDark ? 'text-blue-400' : 'text-blue-600'}`}>
            {claimable?.reward?.toFixed(2) || '0.00'} CFLY
          </div>
        </div>

        <div
          className={`rounded-lg border p-4 text-center ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-purple-900/50 to-purple-800/50'
              : 'border-gray-200 bg-gradient-to-br from-purple-50 to-purple-100'
          }`}
        >
          <Clock className={`mx-auto mb-2 h-6 w-6 ${isDark ? 'text-purple-400' : 'text-purple-600'}`} />
          <div className={`text-sm ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>Days Staked</div>
          <div className={`mt-1 text-xl font-bold ${isDark ? 'text-purple-400' : 'text-purple-600'}`}>
            {claimable?.days || 0}
          </div>
        </div>

        <div
          className={`rounded-lg border p-4 text-center ${
            isDark
              ? 'border-gray-700 bg-gradient-to-br from-green-900/50 to-green-800/50'
              : 'border-gray-200 bg-gradient-to-br from-green-50 to-green-100'
          }`}
        >
          <TrendingUp className={`mx-auto mb-2 h-6 w-6 ${isDark ? 'text-green-400' : 'text-green-600'}`} />
          <div className={`text-sm ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>APY</div>
          <div className={`mt-1 text-xl font-bold ${isDark ? 'text-green-400' : 'text-green-600'}`}>
            {apy?.toFixed(2) || '0.00'}%
          </div>
        </div>
      </div>

      {/* Actions */}
      {!account ? (
        <div
          className={`rounded-lg border p-8 text-center ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <Wallet className={`mx-auto mb-4 h-12 w-12 ${isDark ? 'text-gray-600' : 'text-gray-400'}`} />
          <h3 className={`mb-2 text-xl font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            Connect Your Wallet
          </h3>
          <p className={`mb-4 ${isDark ? 'text-gray-400' : 'text-gray-600'}`}>
            Connect your Kadena wallet to manage staking and claim rewards
          </p>
          <button
            onClick={() => initializeKadenaWallet('eckoWallet')}
            className="inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-blue-500 to-purple-600 px-6 py-3 font-semibold text-white transition-transform hover:scale-105"
          >
            <Wallet className="h-5 w-5" />
            Connect Wallet
          </button>
        </div>
      ) : (
        <div
          className={`rounded-lg border p-6 ${
            isDark ? 'border-gray-700 bg-gray-800' : 'border-gray-200 bg-white'
          }`}
        >
          <h3 className={`mb-4 text-lg font-bold ${isDark ? 'text-white' : 'text-gray-900'}`}>
            Node Management Actions
          </h3>
          <div className="flex flex-wrap gap-4">
            {canStake ? (
              <button
                onClick={handleStake}
                disabled={actionLoading}
                className="inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-green-500 to-emerald-600 px-6 py-3 font-semibold text-white transition-transform hover:scale-105 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <ArrowUp className="h-5 w-5" />
                {actionLoading ? 'Processing...' : 'Stake 50,000 CFLY'}
              </button>
            ) : (
              <>
                <button
                  onClick={handleUnstake}
                  disabled={actionLoading}
                  className="inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-red-500 to-pink-600 px-6 py-3 font-semibold text-white transition-transform hover:scale-105 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <ArrowDown className="h-5 w-5" />
                  {actionLoading ? 'Processing...' : 'Unstake'}
                </button>
                {claimable && claimable.reward > 0 && (
                  <button
                    onClick={handleClaim}
                    disabled={actionLoading}
                    className="inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-blue-500 to-cyan-600 px-6 py-3 font-semibold text-white transition-transform hover:scale-105 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <Gift className="h-5 w-5" />
                    {actionLoading ? 'Processing...' : `Claim ${claimable.reward.toFixed(2)} CFLY`}
                  </button>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
