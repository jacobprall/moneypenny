# == Schema Information
#
# Table name: accounts
#
#  id               :bigint           not null, primary key
#  account_category :string           not null
#  balance          :float            not null
#  debit            :boolean          not null
#  institution      :string           not null
#  label            :string           not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null
#  user_id          :integer          not null
#
# Indexes
#
#  index_accounts_on_user_id  (user_id)
#
require 'test_helper'

class AccountTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
