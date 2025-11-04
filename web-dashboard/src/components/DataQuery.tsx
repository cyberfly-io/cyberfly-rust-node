import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { 
  getDataByDbName, 
  getDataByDbNameAndType,
  type DataEntry 
} from '../api/client';
import { loadKeyPair } from '../utils/crypto';

export default function DataQuery() {
  const [storeType, setStoreType] = useState('');
  const [dbName, setDbName] = useState('');

  // Auto-load dbName from keypair
  const keyPair = loadKeyPair();
  const defaultDbName = keyPair ? `mydb-${keyPair.publicKey}` : '';

  const dbQuery = useQuery({
    queryKey: ['dbData', dbName, storeType],
    queryFn: () => {
      const targetDbName = dbName || defaultDbName;
      if (!targetDbName) {
        throw new Error('No database name specified');
      }
      if (storeType) {
        return getDataByDbNameAndType(targetDbName, storeType);
      }
      return getDataByDbName(targetDbName);
    },
    enabled: (dbName.length > 0 || defaultDbName.length > 0),
  });

  const data = dbQuery.data || [];

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-6 pr-48">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white dark:text-gray-100">Query Data</h1>
        <button
          onClick={() => dbQuery.refetch()}
          className="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition"
        >
          <RefreshCw className="w-4 h-4" />
          Refresh
        </button>
      </div>

      {/* Query Controls */}
      <div className="bg-white dark:bg-gray-800 dark:bg-gray-800 rounded-lg shadow p-6 mb-6">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Database Name */}
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 dark:text-gray-300 mb-2">
              Database Name
            </label>
            <input
              type="text"
              value={dbName}
              onChange={(e) => setDbName(e.target.value)}
              placeholder={defaultDbName || 'mydb-publickey'}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm"
            />
            {defaultDbName && !dbName && (
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
                Using your keypair's database: {defaultDbName.substring(0, 30)}...
              </p>
            )}
          </div>

          {/* Store Type Filter */}
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 dark:text-gray-300 mb-2">
              Store Type (optional)
            </label>
            <select
              value={storeType}
              onChange={(e) => setStoreType(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="">All Types</option>
              <option value="String">String</option>
              <option value="Hash">Hash</option>
              <option value="List">List</option>
              <option value="Set">Set</option>
              <option value="SortedSet">Sorted Set</option>
              <option value="Json">JSON</option>
              <option value="Stream">Stream</option>
              <option value="TimeSeries">Time Series</option>
              <option value="Geo">Geospatial</option>
            </select>
          </div>
        </div>
      </div>

      {/* Results */}
      <div className="bg-white dark:bg-gray-800 dark:bg-gray-800 rounded-lg shadow">
        <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-lg font-semibold">
            Results ({data.length})
            {dbQuery.isFetching && (
              <span className="ml-2 text-sm text-gray-500 dark:text-gray-400">Loading...</span>
            )}
          </h2>
        </div>

        <div className="divide-y divide-gray-200 max-h-[600px] overflow-y-auto">
          {data.length === 0 ? (
            <div className="p-8 text-center text-gray-500 dark:text-gray-400">
              {dbQuery.isFetching ? 'Loading...' : 'No data found'}
            </div>
          ) : (
            data.map((entry, index) => (
              <DataEntryRow key={`${entry.key}-${index}`} entry={entry} />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function DataEntryRow({ entry }: { entry: DataEntry }) {
  const [expanded, setExpanded] = useState(false);

  const getStoreTypeColor = (type: string) => {
    const colors: Record<string, string> = {
      String: 'bg-blue-100 text-blue-800',
      Hash: 'bg-purple-100 text-purple-800',
      List: 'bg-green-100 text-green-800',
      Set: 'bg-yellow-100 text-yellow-800',
      SortedSet: 'bg-orange-100 text-orange-800',
      Json: 'bg-pink-100 text-pink-800',
      Stream: 'bg-indigo-100 text-indigo-800',
      TimeSeries: 'bg-red-100 text-red-800',
      Geo: 'bg-teal-100 text-teal-800',
    };
    return colors[type] || 'bg-gray-100 dark:bg-gray-800 text-gray-800 dark:text-gray-200';
  };

  return (
    <div className="p-4 hover:bg-gray-50 dark:bg-gray-700 dark:bg-gray-700 transition">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-2">
            <span className={`px-2 py-1 rounded text-xs font-medium ${getStoreTypeColor(entry.storeType)}`}>
              {entry.storeType}
            </span>
            <code className="text-sm font-mono text-gray-700 dark:text-gray-300 dark:text-gray-300">{entry.key}</code>
          </div>

          <div className="text-sm text-gray-600 dark:text-gray-400 dark:text-gray-400">
            {expanded ? (
              <pre className="mt-2 p-3 bg-gray-50 dark:bg-gray-700 dark:bg-gray-700 rounded overflow-x-auto">
                {JSON.stringify(entry.value, null, 2)}
              </pre>
            ) : (
              <p className="truncate">
                {typeof entry.value === 'object' 
                  ? JSON.stringify(entry.value)
                  : String(entry.value)
                }
              </p>
            )}
          </div>

          {entry.metadata && expanded && (
            <div className="mt-3 p-3 bg-blue-50 rounded text-xs">
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <span className="font-medium">Public Key:</span>
                  <code className="block mt-1 text-gray-700 dark:text-gray-300 dark:text-gray-300">{entry.metadata.publicKey}</code>
                </div>
                <div>
                  <span className="font-medium">Timestamp:</span>
                  <p className="mt-1">{new Date(entry.metadata.timestamp).toLocaleString()}</p>
                </div>
              </div>
            </div>
          )}
        </div>

        <button
          onClick={() => setExpanded(!expanded)}
          className="ml-4 px-3 py-1 text-xs bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-600 rounded transition"
        >
          {expanded ? 'Collapse' : 'Expand'}
        </button>
      </div>
    </div>
  );
}
