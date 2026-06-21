import React, { createContext, useContext, useState, useCallback } from "react";
import { CheckCircle2, XCircle, AlertCircle, X } from "lucide-react";

type ToastType = "success" | "error" | "info";

interface Toast {
  id: string;
  type: ToastType;
  message: string;
}

interface ToastContextType {
  showToast: (type: ToastType, message: string) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return context;
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const showToast = useCallback((type: ToastType, message: string) => {
    const id = Math.random().toString(36).substring(2, 9);
    setToasts((prev) => [...prev, { id, type, message }]);
    
    // Auto-remove after 3 seconds
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 3000);
  }, []);

  const removeToast = (id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  };

  return (
    <ToastContext.Provider value={{ showToast }}>
      {children}
      <div className="fixed top-4 right-4 z-50 flex flex-col gap-2.5 max-w-md w-full pointer-events-none">
        {toasts.map((toast) => {
          const icons = {
            success: <CheckCircle2 className="text-green-500 shrink-0 animate-bounce" size={18} />,
            error: <XCircle className="text-red-500 shrink-0" size={18} />,
            info: <AlertCircle className="text-blue-500 shrink-0" size={18} />,
          };
          
          const bgColors = {
            success: "bg-white dark:bg-gray-800 border-green-100 dark:border-green-950 border-l-4 border-l-green-500",
            error: "bg-white dark:bg-gray-800 border-red-100 dark:border-red-950 border-l-4 border-l-red-500",
            info: "bg-white dark:bg-gray-800 border-blue-100 dark:border-blue-950 border-l-4 border-l-blue-500",
          };

          return (
            <div
              key={toast.id}
              className={`flex items-start gap-3 p-3.5 rounded-xl border shadow-lg pointer-events-auto transform transition-all duration-300 translate-x-0 ${bgColors[toast.type]}`}
            >
              {icons[toast.type]}
              <div className="flex-1 text-sm font-medium text-gray-800 dark:text-gray-200 break-words leading-relaxed font-sans">
                {toast.message}
              </div>
              <button
                onClick={() => removeToast(toast.id)}
                className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition shrink-0"
              >
                <X size={14} />
              </button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}
