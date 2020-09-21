class Api::TransactionsController < ApplicationController
  def index 
    @transactions = current_user.transactions 
    render :index
  end

  def create 
    @transaction = Transaction.create(transaction_params)
    if @transaction.save
      @transaction.update_account
      render 'api/transactions/update'
    else
      render json: @transaction.errors.full_messages, status: 422
    end
  end

  def update 
    @transaction = Transaction.find(params[:id])
    old_amount = @transaction.amount
    if @transaction.update(transaction_params)
      @transaction.update_on_change(old_amount)
      render 'api/transactions/update'
    else
      render json: @transaction.errors.full_messages, status: 422
    end
  end

  def destroy 
    @transaction = current_user.transactions.find(params[:id])
    @transaction.update_on_delete
    @transaction.destroy
    @transactions = current_user.transactions
    render json: @transaction.id
  end

  def search
    @transactions = Transaction.search_for_transaction(params[:search_params])
    print @transactions
    render json: @transactions
  end

  def transaction_params 
    params.require(:transaction).permit(:amount, :date, :description, :tags, :transaction_category, :account_id)
  end
  
end


 