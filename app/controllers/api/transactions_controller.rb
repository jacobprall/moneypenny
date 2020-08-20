class Api::TransactionsController < ApplicationController
  def index 
    @transactions = current_user.transactions 
    render 'api/transactions/show'
  end

  def create 
    @transaction = Transaction.create(transaction_params)
    if @transaction.save! 
      render 'api/transactions/show'
    else
      render json: @transaction.errors.full_messages 
    end
  end

  def update 
    @transaction = Transaction.find(params[:id])
    if @transaction.update(transaction_params)
      render 'api/transactions/show'
    else
      render json: @transaction.errors.full_messages, status: 422
    end
  end

  def destroy 
    @transaction = current_user.transactions.find(params[:id])
    @transaction.destroy
    render 'api/transactions/show'
  end

  def transaction_params 
    params.require(:transaction).permit(:amount, :date, :description, :tags, :transaction_category, :account_id)
  end
  
end


 