import { useState } from 'react';
import { BrowserRouter, Routes, Route, NavLink, useParams, useNavigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { LayoutDashboard, Database, Search, HardDrive, Key, Menu, X, Settings, Users, Sun, Moon, Activity, Wallet, Cloud, Network } from 'lucide-react';
import Dashboard from './components/Dashboard';
import DataSubmit from './components/DataSubmit';
import DataQuery from './components/DataQuery';
import BlobManager from './components/BlobManager';
import { KeyPairManager } from './components/KeyPairManager';
import { SettingsModal } from './components/Settings';
import PeerConnection from './components/PeerConnection';
import Metrics from './components/Metrics';
import MyNodes from './components/MyNodes';
import AllNodes from './components/AllNodes';
import NodeDetails from './components/NodeDetails';
import { ThemeProvider, useTheme } from './context/ThemeContext';
import { KadenaWalletProvider, useKadenaWallet } from './context/KadenaWalletContext';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

// Wrapper for NodeDetails that uses URL params
function NodeDetailsRoute() {
  const { peerId } = useParams<{ peerId: string }>();
  const navigate = useNavigate();
  
  if (!peerId) {
    navigate('/all-nodes');
    return null;
  }

  return <NodeDetails peerId={peerId} onBack={() => navigate(-1)} />;
}

// Wrapper for MyNodes with navigation
function MyNodesRoute() {
  const navigate = useNavigate();
  return <MyNodes onNodeClick={(peerId) => navigate(`/node/${peerId}`)} />;
}

// Wrapper for AllNodes with navigation
function AllNodesRoute() {
  const navigate = useNavigate();
  return <AllNodes onNodeClick={(peerId) => navigate(`/node/${peerId}`)} />;
}

function AppContent() {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { theme, toggleTheme } = useTheme();
  const { account, initializeKadenaWallet, disconnectWallet, isInstalled } = useKadenaWallet();

  const navigation = [
    { path: '/', name: 'Dashboard', icon: LayoutDashboard },
    { path: '/metrics', name: 'Metrics', icon: Activity },
    { path: '/my-nodes', name: 'My Nodes', icon: Cloud },
    { path: '/all-nodes', name: 'All Nodes', icon: Network },
    { path: '/keypair', name: 'KeyPair', icon: Key },
    { path: '/submit', name: 'Store Data', icon: Database },
    { path: '/query', name: 'Query Data', icon: Search },
    { path: '/blobs', name: 'Blob Storage', icon: HardDrive },
    { path: '/peers', name: 'Connect Peer', icon: Users },
  ];

  return (
    <div className="min-h-screen bg-gradient-to-br from-gray-50 via-blue-50 to-purple-50 dark:from-gray-900 dark:via-gray-800 dark:to-gray-900">
      {/* Sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 z-50 w-64 glass dark:glass-dark shadow-2xl transform transition-all duration-300 ease-in-out border-r border-white/20 dark:border-gray-700/50 backdrop-blur-2xl ${
          sidebarOpen ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        <div className="flex items-center justify-between p-6 border-b border-white/20 dark:border-gray-700/50 bg-gradient-to-r from-blue-600 to-purple-600">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-white/20 rounded-lg backdrop-blur-sm">
              <Cloud className="w-6 h-6 text-white" />
            </div>
            <h1 className="text-2xl font-bold text-white">CyberFly</h1>
          </div>
          <button
            onClick={() => setSidebarOpen(false)}
            className="lg:hidden p-2 rounded-lg hover:bg-white/20 transition-colors backdrop-blur-sm"
          >
            <X className="w-5 h-5 text-white" />
          </button>
        </div>

          <nav className="p-4 space-y-2">
            {navigation.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.path}
                  to={item.path}
                  onClick={() => {
                    // Only close sidebar on mobile
                    if (window.innerWidth < 1024) {
                      setSidebarOpen(false);
                    }
                  }}
                  className={({ isActive }) => `group w-full flex items-center gap-3 px-4 py-3 rounded-xl transition-all duration-200 ${
                    isActive
                      ? 'bg-gradient-to-r from-blue-600 to-blue-700 text-white shadow-xl transform scale-105 backdrop-blur-sm'
                      : 'text-gray-700 dark:text-gray-300 hover:glass dark:hover:glass-dark hover:backdrop-blur-md hover:transform hover:scale-102'
                  }`}
                >
                  <Icon className="w-5 h-5 group-hover:scale-110 transition-transform" />
                  <span className="font-semibold">{item.name}</span>
                </NavLink>
              );
            })}
          </nav>

          <div className="absolute bottom-0 left-0 right-0 p-6 border-t border-white/20 dark:border-gray-700/50 glass dark:glass-dark backdrop-blur-xl">
            <div className="text-sm text-gray-600 dark:text-gray-400">
              <p className="font-bold mb-2 text-gray-900 dark:text-white">Rust Backend</p>
              <p className="mb-1">Version 0.1.0</p>
              <p className="text-xs opacity-75">Iroh Network + Sled DB</p>
            </div>
          </div>
        </aside>

        {/* Mobile menu button */}
        <button
          onClick={() => setSidebarOpen(true)}
          className={`lg:hidden fixed top-4 left-4 z-40 p-3 bg-gradient-to-r from-blue-600 to-purple-600 text-white rounded-xl shadow-2xl hover:shadow-xl transition-all duration-300 hover:scale-110 ${
            sidebarOpen ? 'hidden' : 'block'
          }`}
        >
          <Menu className="w-6 h-6 dark:text-gray-300" />
        </button>

        {/* Header buttons container */}
        <div className="fixed top-6 right-6 z-40 flex items-center gap-3">
          {/* Wallet button */}
          {isInstalled && (
            <button
              onClick={() => account ? disconnectWallet() : initializeKadenaWallet('eckoWallet')}
              className={`px-4 py-3 rounded-xl shadow-xl hover:shadow-2xl transition-all duration-300 hover:scale-105 flex items-center gap-2 font-semibold backdrop-blur-md ${
                account 
                  ? 'bg-gradient-to-r from-green-500 to-green-600 text-white' 
                  : 'bg-gradient-to-r from-blue-600 to-purple-600 text-white'
              }`}
              title={account ? 'Disconnect wallet' : 'Connect wallet'}
            >
              <Wallet className="w-5 h-5" />
              {account && (
                <span className="hidden sm:inline text-sm font-mono">
                  {account.slice(0, 6)}...{account.slice(-4)}
                </span>
              )}
            </button>
          )}

          {/* Theme toggle button */}
          <button
            onClick={toggleTheme}
            className="p-3 glass dark:glass-dark rounded-xl shadow-xl hover:shadow-2xl transition-all duration-300 hover:scale-110 border border-white/30 dark:border-gray-700/50 backdrop-blur-md"
            title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          >
            {theme === 'dark' ? (
              <Sun className="w-5 h-5 text-yellow-500" />
            ) : (
              <Moon className="w-5 h-5 text-blue-600" />
            )}
          </button>

          {/* Settings button */}
          <button
            onClick={() => setSettingsOpen(true)}
            className="p-3 glass dark:glass-dark rounded-xl shadow-xl hover:shadow-2xl transition-all duration-300 hover:scale-110 border border-white/30 dark:border-gray-700/50 backdrop-blur-md"
            title="Settings"
          >
            <Settings className="w-5 h-5 text-gray-700 dark:text-gray-300" />
          </button>
        </div>

        {/* Main content */}
        <main
          className={`transition-all duration-200 ${
            sidebarOpen ? 'lg:ml-64' : 'ml-0'
          }`}
        >
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/metrics" element={<Metrics />} />
            <Route path="/my-nodes" element={<MyNodesRoute />} />
            <Route path="/all-nodes" element={<AllNodesRoute />} />
            <Route path="/node/:peerId" element={<NodeDetailsRoute />} />
            <Route path="/keypair" element={<KeyPairManager />} />
            <Route path="/submit" element={<DataSubmit />} />
            <Route path="/query" element={<DataQuery />} />
            <Route path="/blobs" element={<BlobManager />} />
            <Route path="/peers" element={<PeerConnection />} />
          </Routes>
        </main>

        {/* Settings Modal */}
        <SettingsModal isOpen={settingsOpen} onClose={() => setSettingsOpen(false)} />

        {/* Overlay for mobile */}
        {sidebarOpen && (
          <div
            className="lg:hidden fixed inset-0 bg-black bg-opacity-50 z-40"
            onClick={() => setSidebarOpen(false)}
          />
        )}
      </div>
  );
}

function App() {
  return (
    <BrowserRouter>
      <ThemeProvider>
        <KadenaWalletProvider>
          <QueryClientProvider client={queryClient}>
            <AppContent />
          </QueryClientProvider>
        </KadenaWalletProvider>
      </ThemeProvider>
    </BrowserRouter>
  );
}

export default App;
