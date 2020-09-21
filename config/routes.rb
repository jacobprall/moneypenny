Rails.application.routes.draw do
  
  namespace :api, defaults: { format: :json } do
    resources :users, only: [:create, :show]
    resource :session, only: [:create, :destroy]
    resources :accounts, except: [:edit, :new]
    resources :transactions, except: [:edit, :new]
    resources :goals, except: [:edit, :new]
    resources :bills, except: [:edit, :new]
    get 'transactions/search/:search_params', to: 'transactions#search'
  end

  root to: 'static_pages#root'
end
