class Api::TransactionsController < ApplicationController
  def index 
    @transactions = current_user.transactions 
    render :index
  end

  def create 
    @transaction = Transaction.create(transaction_params)
    if @transaction.save
      render 'api/transactions/update'
    else
      render json: @transaction.errors.full_messages 
    end
  end

  def update 
    @transaction = Transaction.find(params[:id])
    if @transaction.update(transaction_params)
      @transaction.update_account(@transaction.amount)
      render 'api/transactions/update'
    else
      render json: @transaction.errors.full_messages, status: 422
    end
  end

  def destroy 
    @transaction = current_user.transactions.find(params[:id])
    @transaction.destroy
    render 'api/transactions/index'
  end

  def transaction_params 
    params.require(:transaction).permit(:amount, :date, :description, :tags, :transaction_category, :account_id)
  end
  
end


 