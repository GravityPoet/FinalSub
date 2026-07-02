import { forwardRef } from "react";

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className = "", ...props }, ref) => {
    return (
      <input
        ref={ref}
        className={`glass-control h-10 w-full rounded-xl px-3.5 py-2 text-sm text-text-primary placeholder:text-text-tertiary transition focus:border-brand focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:opacity-50 ${className}`}
        {...props}
      />
    );
  }
);
Input.displayName = "Input";

export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ className = "", children, ...props }, ref) => {
    return (
      <select
        ref={ref}
        className={`glass-control h-10 w-full rounded-xl px-3.5 py-2 text-sm text-text-primary transition focus:border-brand focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:opacity-50 ${className}`}
        {...props}
      >
        {children}
      </select>
    );
  }
);
Select.displayName = "Select";

export interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className = "", ...props }, ref) => {
    return (
      <textarea
        ref={ref}
        className={`glass-control w-full rounded-xl px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-tertiary transition focus:border-brand focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:opacity-50 ${className}`}
        {...props}
      />
    );
  }
);
Textarea.displayName = "Textarea";
