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
            success: <CheckCircle2 className="text-success shrink-0 animate-bounce" size={18} />,
            error: <XCircle className="text-danger shrink-0" size={18} />,
            info: <AlertCircle className="text-info shrink-0" size={18} />,
          };
          
          const bgColors = {
            success: "bg-surface border-border-subtle border-l-4 border-l-success",
            error: "bg-surface border-border-subtle border-l-4 border-l-danger",
            info: "bg-surface border-border-subtle border-l-4 border-l-info",
          };

          return (
            <div
              key={toast.id}
              className={`flex items-start gap-3 p-3.5 rounded-xl border shadow-lg pointer-events-auto transform transition-all duration-300 translate-x-0 ${bgColors[toast.type]}`}
            >
              {icons[toast.type]}
              <div className="flex-1 text-sm font-medium text-text-primary break-words leading-relaxed font-sans">
                {toast.message}
              </div>
              <button
                onClick={() => removeToast(toast.id)}
                className="text-text-tertiary hover:text-text-primary transition shrink-0"
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
